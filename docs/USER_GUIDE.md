# Floe User Guide

This guide explains features available in the current development version of
Floe. Labels and menu placement may evolve. The
[feature matrix](./FEATURE_MATRIX.md) remains the source of truth for what is
complete, partial, planned, or deferred.

## Start Floe

From a source checkout:

```bash
cargo run -p floe-app
```

Floe is a single-instance application. Running the command again normally
activates the existing window instead of starting another independent process.

## Find any command

Press `Ctrl+Shift+P` to open the Command Palette. Search by a human-readable
name such as “Split View”, “Calculate Checksums”, “Open Terminal Here”, or
“Protected Folders”. The palette uses the same live actions as the toolbar and
context menus, so commands that do not apply to the current selection are
shown as unavailable rather than running with the wrong target.

Press `Ctrl+?` or choose **Keyboard Shortcuts…** from the three-dot header menu
to browse every command, search its description, change its shortcut, reset
one shortcut, or restore all defaults.

## The main window

- The header contains Back, Forward, Parent, the editable location, Search,
  view controls, and the three-dot **Main menu**.
- The left sidebar contains standard XDG Places, bookmarks, and available
  drives, volumes, and mounts.
- The central browser can use List, Grid, or Miller/Columns presentation.
- The Operations Island appears while work is running and reports progress,
  cancellation, conflicts, failures, retry actions, and completion.
- Tabs appear above the browser. Split View adds a second independently
  navigable pane.

Most actions are available in more than one place: a right-click menu, the
three-dot header menu, the Command Palette, or a keyboard shortcut.

The Main menu uses task groups instead of one long option list:

- **Create** — folders, empty files, and templates.
- **Open & Inspect** — Open With, terminal choices, and Properties.
- **File Operations** — Transfer, Rename & Duplicate, Links, Copy Details,
  and Trash.
- **View & Layout** — sidebar, appearance, icons, browser view, and split view.
- **Tools & Safety** — checksums, Protected Folders, integrity, and archives.

Context-menu customization, operation history, keyboard shortcuts, and desktop
integration remain in the final utility section.

## Navigate folders

| Task | How |
| --- | --- |
| Open a folder | Double-click it, select it and press `Enter`, or use **Open** |
| Go back / forward | `Alt+Left` / `Alt+Right` |
| Go to the parent folder | `Alt+Up` |
| Enter a path directly | Press `Ctrl+L`, enter an absolute path, then press `Enter` |
| Cancel location editing | Press `Escape` |
| Refresh the folder | Use **Refresh** from the background menu or Command Palette |
| Show hidden files | Press `Ctrl+H` or use the hidden-files header control |

The location field always navigates using the original filesystem path. Text
shown for an unusual Linux filename may be lossy, but Floe does not rebuild the
real path from that display text.

### Places, bookmarks, and devices

- Click a Place or bookmark to navigate to it.
- Add the current folder as a bookmark from the sidebar controls or applicable
  context action.
- Drag the sidebar divider to resize it. Floe remembers the width.
- Use **Sidebar Density** in the header menu to choose Compact, Balanced, or
  Comfortable spacing.
- Mounted local devices can be opened from **Devices**. Available mount,
  unmount, and eject actions use GIO and native password prompts where the
  desktop provides them.
- Use **Desktop Integration…** to see which standard services are available or
  limited in the current desktop session.

Remote browsing and Android/MTP workflows are currently deferred. A local
mount exposed by the desktop can still appear as an ordinary mounted device.

## Choose a view

- Press `Ctrl+1` for List View.
- Press `Ctrl+2` for Grid View.
- Choose Miller/Columns from the view controls or Command Palette for spatial
  parent-to-child navigation.
- In Grid View, use the minus/plus controls or `Ctrl+-` / `Ctrl++` to change
  icon size.
- Use the view menu to change sort column, direction, grouping, folder
  placement, file-view density, and optional list columns.
- Column widths, sidebar width, per-tab view state, and other implemented view
  preferences are restored across clean launches.

Floe can remember view settings per folder. If grouping is enabled in Grid
View, group headings are part of the grid rather than a separate list-only
model.

## Select files and folders

- Click an item for a single selection.
- Hold `Ctrl` while clicking to add or remove individual items.
- Hold `Shift` to select a range.
- Drag across empty Grid View space for rubber-band selection.
- Press `Ctrl+A` to select everything.
- Press `Ctrl+Shift+A` or `Escape` to clear the selection when the file view has
  focus.
- Press `Shift+F10` or the Menu key to open the context menu from the keyboard.

Right-clicking an unselected item targets that item. Right-clicking an item
already inside a multi-selection preserves the full selection. Right-clicking
empty browser space opens the folder-background menu with creation, Paste,
Refresh, Select All, and location actions.

Choose **Customize Context Menus…** to show or hide the reviewed optional
groups. Essential actions such as Properties, recovery, destructive actions,
and access to customization remain reachable.

## Open files and choose applications

- **Open** uses the registered default application through GIO.
- If no default exists, Floe opens the application chooser instead of silently
  failing.
- **Open With…** chooses an application for this launch.
- **Set as Default** is an explicit separate choice; using Open With does not
  silently change the file association.

Applications are launched with native file/URI APIs. Floe does not interpolate
filenames into shell commands.

## Create and organize files

Right-click empty folder space or use the Command Palette to create a folder,
empty file, FIFO, or item from a template. New items use validated names and do
not overwrite an existing entry.

| Task | How |
| --- | --- |
| Copy | Select items, press `Ctrl+C`, open the destination, press `Ctrl+V` |
| Move | Select items, press `Ctrl+X`, open the destination, press `Ctrl+V` |
| Rename | Select one item and press `F2` |
| Batch rename | Select multiple items and choose **Batch Rename…** |
| Duplicate | Choose **Duplicate** from the context menu or Command Palette |
| Create links | Use **Create Symbolic Link** or **Create Hard Link** where eligible |
| Drag and drop | Drag selected items to a folder, Place, bookmark, device, pane, or Miller column |

Split View offers commands to copy, move, open, or link items to the opposite
pane. Floe never silently replaces an existing destination. A conflict surface
offers only reviewed choices safe for the current operation, such as Keep
Existing, Keep Both, or Retry With New Name.

### Operation progress and history

The Operations Island is non-modal: browsing can continue while a job runs.
It shows determinate progress when totals are known, measured speed and ETA
when meaningful, cancellation, batch pause/resume boundaries, Retry, and
conflict recovery.

Choose **Operation History…** to review the bounded in-session history and use
available safe Undo actions. Persistent crash recovery is the next planned
milestone; current operation history is not yet a durable recovery journal.

## Trash and permanent deletion

- Press `Delete` or choose **Move to Trash** for ordinary recoverable removal.
- Open Trash from the header menu or Command Palette.
- Select Trash items and choose **Restore** to return them without overwriting.
- **Empty Trash…** permanently removes all Trash items after confirmation.
- `Shift+Delete` opens the explicit permanent-deletion confirmation.

Permanent deletion is normal filesystem removal, not secure erase. SSD
firmware, snapshots, copy-on-write storage, backups, and external copies may
retain data.

## Search and filter

Press `Ctrl+F` to open one unified Search surface. Choose the mode that matches
the task:

- **Quick Filter** narrows the items already shown in the current folder.
- **Search Files** recursively searches filenames under the selected scope.
- **Search Contents** reads eligible local text files within strict limits and
  reports matching lines.

Text mode performs a normal text match. Glob mode uses patterns such as
`*.pdf` or `photo-??.jpg`. Regex mode accepts an advanced regular expression
such as `^invoice-[0-9]+\.pdf$`. Spaces are ordinary searchable characters in
the search entry.

Advanced filters can narrow by type, extension, MIME type, size, date, owner,
hidden state, and case sensitivity. Search Files and Search Contents can save
queries, show recent searches, and optionally use the private filename/metadata
index when eligible. Hidden entries and file contents are never put in that
index; Floe falls back to complete live search when an index is unavailable or
stale.

Use **Reveal in Folder** on a result to navigate to its exact parent and select
the exact result.

### Find duplicate files

Choose **Check for Duplicates…** from the background or selection context menu,
the File Tools menu, or the command palette. The setup window offers three
workflows:

- **All duplicates in a folder tree:** choose a folder to find every exact
  duplicate group inside it and all subfolders.
- **Copies of the selected file:** select one regular file, choose a folder,
  and find exact copies of that file anywhere below the chosen folder.
- **Selected files and folders:** compare the explicit selection; every
  selected folder is scanned recursively.

With no selection, Floe defaults to the current folder tree. One selected file
defaults to finding copies of that file. One selected folder defaults to that
folder tree. Multiple supported items default to the explicit selection.

For every workflow, Floe:

1. groups candidates by exact size;
2. compares bounded first and last samples (up to 64 KiB each) to reject most
   same-size nonmatches cheaply;
3. calculates or validates a cached SHA-256 only for remaining candidates;
4. confirms matching candidates byte-for-byte;
5. revalidates files that may have changed during the scan.

The progress text names the real stage: discovering, quick filtering, hashing,
or byte confirmation. Hashing uses at most four workers and no more than two
active reads per filesystem device, so scanning can use multiple drives without
starting an unbounded number of reads.

The first scan of new or changed candidates is a **cold scan** and must calculate
their full SHA-256 digests. Later **warm scans** can reuse a digest only when the
exact raw path, device, inode, size, modification time, and change time still
match. Floe invalidates watched changed paths and descendants, and every scan
also revalidates files before accepting a cache hit. A warm hit avoids full
rehashing; it never skips the quick sample or final byte-for-byte confirmation.

The derived cache lives under the XDG cache directory at
`floe/duplicate-hashes-v1` (normally
`~/.cache/floe/duplicate-hashes-v1`). It contains paths, file identity/timestamps,
SHA-256 digests, and bounded recency—not file content, duplicate groups, scan
history, or deletion choices. Removing it is safe and only makes the next scan
cold. See `docs/PRIVACY_SECURITY.md` before scanning sensitive paths because
Private Mode and Sensitive Folder cache suppression are later phases.

Hard-link aliases are identified separately and are not counted as reclaimable
duplicate bytes. Symbolic links are not followed, mounted filesystems below the
chosen root are not crossed, results remain local and memory-only, and scanning
can be cancelled. Floe never deletes duplicate results automatically; review
and remove them through the normal Trash or deletion actions.

“Exact duplicate” means identical bytes. Two videos that look the same but were
re-encoded or contain different container metadata are not exact duplicates;
similar-media detection remains a separate future capability.

## Quick Preview, Inspector, and Properties

- Select one file and press `Space` to toggle Quick Preview.
- In Miller/Columns View, press `Ctrl+I` to toggle the Inspector final column.
- Press `Alt+Enter` or choose **Properties** for the full properties surface.

Quick Preview supports implemented image, bounded text/source, PDF/document,
audio/video, font, and archive-listing providers. Unsupported or malformed
content produces a passive error; previewing does not execute the file.

The Inspector provides lightweight selection details and metadata. Properties
adds filesystem details, aggregate folder facts, Open With information,
permissions, and advanced metadata where available. Permission editing is an
explicit background operation with risk acknowledgement and no-follow checks.

Some document thumbnails and previews depend on installed freedesktop
thumbnailer/provider applications. Those helpers are supervised and bounded,
but currently run with the user's normal authority; provider sandboxing is a
later security phase.

## Archives

- Select files/folders and choose **Compress…** to create a supported archive.
- Select an archive and choose **Extract Here** or **Extract To…**.
- Press `Space` on a supported archive to inspect its bounded member listing
  without extracting it.

Floe rejects unsafe traversal paths, unsafe links, conflicts, and decompression
plans that exceed reviewed limits. Archive operations do not silently
overwrite. Password-protected archives are not currently supported.

## Checksums and integrity tools

Integrity commands are available through the file context menu, header menu,
and Command Palette when the selection is eligible.

### Calculate checksums

Choose **Calculate Checksums…** for SHA-256, SHA-512, or legacy MD5 comparison.
MD5 is provided only for compatibility, not security.

### Saved SHA-256 fingerprint

1. Select one eligible local file.
2. Choose **Save SHA-256 Fingerprint**.
3. Later select that file and choose **Verify Saved Fingerprint**.

Verification reports whether the current identity and bytes still match the
saved record.

### Portable SHA256SUMS manifest

1. Select eligible local files/folders that share a manifest root.
2. Choose **Generate SHA256SUMS**. Floe creates `SHA256SUMS` in that root and
   refuses to replace an existing manifest.
3. Later select that manifest and choose **Verify Selected Manifest**.

The result separates matching, changed, missing, and newly discovered files.
Manifest parsing preserves unusual Linux filename bytes and never treats a
digest as proof of who created the manifest.

### Integrity baselines and monitoring

- **Create Integrity Baseline** records an explicit local tree baseline.
- **Update Integrity Baseline** deliberately replaces the recorded baseline.
- **Verify Integrity Baseline** performs an on-demand comparison.
- **Start Integrity Monitoring** watches an eligible local baseline and
  coalesces changes before verification.
- **Stop Integrity Monitoring** stops that session's monitoring.

Monitoring reports changed, missing, and new paths. It is a local change aid,
not intrusion detection, malware scanning, signature verification, or attacker
protection.

### Copy and Verify

Choose **Copy and Verify…** when you want Floe to copy an item and then compare
revalidated source and destination bytes with SHA-256. Ordinary Copy remains
the faster default and is not silently changed. A failed or interrupted result
clearly distinguishes “not created” from “copied but unverified.”

### Verified removable transfer

For an eligible mounted removable device, **Verified Removable Transfer…**
performs Copy, Verify, filesystem flush, then GIO eject/unmount. Floe says a
device is safe to remove only after the entire revalidated workflow succeeds.

### Protected Folders

Use **Protect Folder** on a local folder to add an accidental-change guardrail.
Destructive operations whose source or destination intersects a protected path
require an additional exact-scope review. Use **Protected Folders…** to inspect
the current list or **Unprotect Folder** to remove a rule.

Protected Folder is not encryption, access control, immutability, or protection
from other applications. It reduces mistakes made through Floe.

Checksums and hashes prove only that compared bytes match a recorded digest.
They do not prove authenticity, safety, ownership, or absence of malware.

## Appearance and customization

Open **Main menu → View & Layout** to configure:

- **Appearance**: Native, Glass, Frosted, Minimal, or Compact.
- **File & Folder Icons**: Floe Color, Phosphor Monochrome, or System Theme.
- **Sidebar Density** and **File View Density**.
- sort, grouping, folder placement, list columns, and per-folder view memory;
- **Customize Context Menus…**;
- **Keyboard Shortcuts…**;
- optional Vim navigation mode.

Choices apply immediately and implemented preferences persist. Glass and
Frosted use real transparent GTK surfaces, but background blur is controlled by
the compositor. Without compositor blur, Floe keeps surfaces readable and does
not fake an expensive blur effect.

Optional Vim mode applies to the browser only. Native text fields, search,
dialogs, and other input controls keep standard editing behavior.

## Common default shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+Shift+P` | Command Palette |
| `Ctrl+?` | Browse/customize Keyboard Shortcuts |
| `Alt+Left` / `Alt+Right` / `Alt+Up` | Back / Forward / Parent |
| `Ctrl+L` | Edit location |
| `Ctrl+F` | Unified Search in Quick Filter mode |
| `Ctrl+Shift+F` | Unified Search in Search Files mode |
| `Ctrl+H` | Toggle hidden files |
| `Ctrl+T` / `Ctrl+W` | New / close tab |
| `Ctrl+Shift+T` | Reopen closed tab |
| `F3` / `F6` | Toggle Split View / switch active pane |
| `Ctrl+1` / `Ctrl+2` | List / Grid View |
| `Ctrl+A` / `Ctrl+Shift+A` | Select all / clear selection |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Copy / Cut / Paste |
| `F2` | Rename |
| `Delete` / `Shift+Delete` | Move to Trash / confirm permanent deletion |
| `Space` | Quick Preview |
| `Ctrl+I` | Inspector final column in Miller/Columns View |
| `Alt+Enter` | Properties |
| `Shift+F10` | Keyboard context menu |

Open **Keyboard Shortcuts…** for the authoritative live list because most
shortcuts can be customized.

## Troubleshooting

### A changed appearance does not show

Close the existing Floe window before testing an environment override. Because
Floe is single-instance, another `cargo run` normally reactivates the existing
process.

```bash
FLOE_APPEARANCE=glass cargo run -p floe-app
```

### Glass is transparent but not blurred

Blur is compositor-dependent. Floe supplies transparent surfaces; it cannot
force every Wayland compositor to blur the wallpaper behind them.

### A thumbnail or document preview is missing

Image previews use Floe's built-in reviewed decoders. PDF, office, video, and
other system-provided thumbnails can depend on installed freedesktop
thumbnailers. Floe falls back to a semantic file icon when no eligible provider
is available or a provider rejects the file.

### A command is disabled

Check the selection and current location. Some commands require exactly one
file, one folder, multiple items, a local path, Trash, a mounted removable
device, or an existing integrity record. The Command Palette explains when a
registered action is currently unavailable.

### Check desktop support

Open **Desktop Integration…** for current GIO, XDG, portal, notification,
mount, theme, Secret Service, and session-lock capability status. Missing an
optional service does not disable ordinary local browsing.

## Important current limitations

- Niri-specific, Plasma-specific, remote/network, and Android/MTP integration
  are deferred; generic GTK/GIO/XDG Wayland behavior remains the active path.
- Open as Administrator remains intentionally unavailable until its privileged
  access design and security gates are implemented.
- Persistent interrupted-operation recovery is planned for Phase 18Y.
- Provider sandboxing, Private Mode, Sensitive Folders, encrypted vaults, and
  portable encryption are planned security work, not current claims.
- Permanent delete is not secure erase.

For development and diagnostic commands, see
[Developing Floe](./DEVELOPMENT.md). For exact implementation status, see the
[Feature Matrix](./FEATURE_MATRIX.md), [Roadmap](./ROADMAP.md), and
[Privacy & Security](./PRIVACY_SECURITY.md).
