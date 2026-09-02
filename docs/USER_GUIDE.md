# Floe User Guide

Release documentation: [Getting Started](./GETTING_STARTED.md) ·
[Installation](./INSTALLATION.md) · [Administration](./ADMINISTRATION.md) ·
[Accessibility](./ACCESSIBILITY.md) · [Recovery](./RECOVERY.md) ·
[Debugging](./DEBUGGING.md) · [Localization](./LOCALIZATION.md) ·
[Security](../SECURITY.md) · [Why Floe works this way](./PHILOSOPHY.md)

This guide explains features available in the current development version of
Floe. Labels and menu placement may evolve. The
[feature matrix](./FEATURE_MATRIX.md) remains the source of truth for what is
complete, partial, planned, or deferred.

## Features and their reasons

Floe documents not only what a feature does, but why it behaves that way. When
a choice is surprising, safety-sensitive, privacy-sensitive, or deliberately
limited, the explanation should identify its purpose, tradeoff, and what it does
not claim. The shared principles and a feature-by-feature rationale are in
[Floe Philosophy](./PHILOSOPHY.md); important explanations are repeated beside
the relevant workflows in this guide.

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

## Change settings

Press `Ctrl+,` or choose **Main menu → Settings…**. The Settings Center keeps
related options in eight plain-language sections: Appearance, Browsing, Views
& Layout, Search & Preview, Operations & Safety, Applications, Shortcuts &
Menus, and Accessibility. Type in **Search settings** to filter by a setting's
name, description, category, or familiar phrases such as “right click”,
“folder icons”, or “reduce animation”.

Appearance preset, System/Light/Dark color scheme, interface font and 75–200%
text scale, reduced motion, icon style, single/double-click opening, default
view, per-folder view memory, Vim navigation, grid size, file/sidebar density,
the optional private filename index, and local ClamAV scan limits apply immediately and continue to use
Floe's existing bounded preferences. **Reset appearance** restores Frosted,
system colors/font, 100% text, and normal motion. Settings does not create a
second configuration store. Detailed editors remain focused:
use their **Open** buttons for Keyboard Shortcuts, Context Menu Contents, the
preferred terminal, operation history, Recovery Center, Protected Folders, and
desktop integration. Irreversible-operation confirmations cannot be disabled.

Floe follows GTK desktop settings for system text, contrast, focus, and
assistive technology. Phase 20B2 adds explicit non-color group/pane cues,
focusable collapsible headings, reduced-motion behavior, and direction-isolated
path text. Translation catalogs, a full Orca walkthrough, and a physical
multi-monitor fractional-scale matrix remain release-hardening work.

## The main window

- The header contains Back, Forward, Parent, the editable location, Search,
  view controls, and the three-dot **Main menu**.
- The left sidebar contains standard XDG Places, bookmarks, and available
  drives, volumes, and mounts.
- The central browser can use List, Grid, or Miller/Columns presentation.
- The Operations Island appears while work is running and reports progress,
  cancellation, conflicts, failures, retry actions, and completion.
- A separate **Background Activity** panel appears below the tabs for read-only
  Properties, Privacy inspection, local ClamAV scans, and metadata sanitization.
  Running rows do not expire when Floe loses focus or you navigate, change selection,
  switch tabs, or switch panes. Terminal rows retain **View Results** or **Reveal**
  until explicitly dismissed.
- Compact tabs appear above the browser; long folder names remain one line and
  show the complete path on hover. Split View adds a second independently
  navigable pane.

Floe remembers the last normal window size after you resize and close it, then
restores that size on the next launch. Maximized and fullscreen dimensions do
not replace the remembered normal size. Window position remains owned by the
Wayland compositor and is not persisted by Floe.

Most actions are available in more than one place: a right-click menu, the
three-dot header menu, the Command Palette, or a keyboard shortcut.

The Main menu uses task groups instead of one long option list:

- **Create** — folders, empty files, and templates.
- **Open & Inspect** — Open With, terminal choices, and Properties.
- **File Operations** — Transfer, Rename & Duplicate, Links, Copy Details,
  and Trash.
- **View & Layout** — sidebar, appearance, icons, browser view, and split view.
- **Tools & Safety** — checksums, Protected Folders, integrity, and archives.

Settings, operation history, keyboard shortcuts, and desktop integration remain
in the final utility section. Context-menu customization is available from the
Settings Center and selection-aware context menus.

## Navigate folders

### Use Floe as a file selector

Applications and scripts can open a dedicated Floe Selection Mode window without
changing or reusing the ordinary Floe window:

```bash
floe --choose-open
floe --choose-open --multiple --initial-directory /path/to/folder
floe --choose-folder --initial-directory /path/to/folder
floe --choose-save --initial-directory /path/to/folder --suggested-name report.txt
```

The footer states **Open File**, **Open Files**, **Select Folder**, or **Save
File** and uses the matching **Open**, **Select**, or **Save** action. Open modes
accept only regular files; Select Folder accepts one selected folder or the
current folder. Save accepts one UTF-8 filename component. If that file exists,
Floe requires an explicit **Replace** decision but does not write the file—the
calling application owns the save operation.

On acceptance Floe writes one exact percent-encoded local `file://` URI per line
to standard output and exits successfully. Cancel, Escape, window close, or
`Ctrl+Q` writes no path and exits nonzero. At most 128 files can be returned from
one Open Files request. Each invocation has an independent window and process;
normal Floe session restoration is not used. Direct Selection Mode remains
useful independently of the optional XDG FileChooser portal backend. Package
installation does not select that backend; administrators opt in as documented
in [Administration](./ADMINISTRATION.md).

Selection Mode is deliberately selection-focused. Navigation, views, search,
preview, and selection remain available, while filesystem mutations, external
tools, administrator access, association/settings editors, and cache/index
writes are disabled. Use the normal Floe window for file-management work.

| Task | How |
| --- | --- |
| Open a folder | Click or double-click according to **Settings → Browsing → Opening behavior**, select it and press `Enter`, or use **Open** |
| Go back / forward | `Alt+Left` / `Alt+Right` |
| Go to the parent folder | `Alt+Up` |
| Open a breadcrumb ancestor | Activate its named segment in the header |
| Enter a path directly | Press `Ctrl+L`, enter an absolute path, choose an exact folder suggestion if useful, then press `Enter` |
| Open recent locations | Press `Alt+Down` or use the clock button beside the breadcrumbs |
| Cancel location editing | Press `Escape` |
| Refresh the folder | Use **Refresh** from the background menu or Command Palette |
| Show hidden files | Press `Ctrl+H` or use the hidden-files header control |

The location field always navigates using the original filesystem path. Text
shown for an unusual Linux filename may be lossy, but Floe does not rebuild the
real path from that display text.

Location completion scans only the parent folder implied by the absolute path,
uses a bounded background worker, and suggests folders only. The recent list is
bounded and deduplicated over Floe's authoritative current/back/forward session
history; it does not create a second history database and therefore follows the
same Private/Sensitive session-persistence policy.

### Open from the command line

An installed `floe` executable, or the development binary, accepts exactly one
local target per invocation:

```bash
floe /path/to/folder
floe /path/to/file.pdf
```

A folder becomes the current location. A regular file opens its parent and is
revealed by exact path after loading. If Floe is already running, the request is
routed to that window. Missing paths, non-local URIs, unsupported file types,
and multiple targets produce explicit feedback.

### Places, bookmarks, and devices

- Click a Place or bookmark to navigate to it.
- Add the current folder as a bookmark from the sidebar controls or applicable
  context action.
- Drag the sidebar divider to resize it. Floe remembers the width.
- Use **Sidebar Density** in the header menu to choose Compact, Balanced, or
  Comfortable spacing.
- Mounted local devices can be opened from **Devices**. Each mounted local row
  shows the device name first and concise available space such as `128.4 GB
  free` beneath it; narrow sidebars shorten either line visually without
  wrapping, and hovering reveals the complete text. Available mount, unmount,
  and eject actions use GIO and native password prompts where the desktop
  provides them. If the desktop supplies no usable volume name, Floe shows a
  filesystem label or a drive/partition fallback instead of a blank row.
- Use **Desktop Integration…** to see which standard services are available or
  limited in the current desktop session.

Remote browsing and Android/MTP workflows are currently deferred. A local
mount exposed by the desktop can still appear as an ordinary mounted device.

## Choose a view

The **Sort files and folders** button sits beside the view controls. The same
menu is available at **Main Menu → View & Layout → Browser View → Sort By**.
Choose Name, Size, Modified, Created, Accessed, Type, Rating, Tags, Comment, or
the Document, Image, Audio, Video, and Other metadata submenus. Direction,
**Folders First**, and **Hidden Files Last**
are independent; hidden-last changes ordering but does not reveal hidden files.

Missing timestamps and metadata stay last. Rating, Tags, and Comment read
existing KDE-compatible local extended attributes only after that sort is
chosen; Floe does not create, edit, log, or persist those values.

Advanced choices explicitly scan only the current local folder on a background
worker. Progress and cache hits appear in the status line; reopen **Sort By** and
choose **Cancel Metadata Scan** to stop. Unsupported formats stay at the end.
Image/EXIF and audio use Floe's built-in reviewed providers; video facts require
an installed `ffprobe`. Word/line counts currently cover bounded UTF-8 text and
source formats, not PDF or office document contents.

Repeated advanced sorts can reuse a private source-fingerprint-validated cache.
Open **Settings → Search & Preview** to disable reuse or clear it, or choose
**Clear Metadata Cache** in Sort By. Clearing changes no files or file metadata.

- Press `Ctrl+1` for List View.
- Press `Ctrl+2` for Grid View.
- Choose Miller/Columns from the view controls or Command Palette for spatial
  parent-to-child navigation.
- In Grid View, use the minus/plus controls or `Ctrl+-` / `Ctrl++` to change
  icon size.
- Use the view menu to change sort column, direction, **Type / Extension / Date /
  Size** grouping, folder placement, file-view density, and optional list
  columns. Unknown date/size metadata stays in a final explicit group.
- Activate a group heading to collapse or expand it. In Grid View each heading
  spans the content width above its file tiles; it never takes the place of a
  file tile. The heading remains keyboard-focusable, announces expanded state,
  and hiding a group does not change its files, exact identities, or selection.
- Under **Columns**, choose **Move Left**, **Move Right**, or **Auto Size** for
  any list column. Autosize samples at most 4,096 loaded entries and every width
  remains clamped.
- Column order/widths, sidebar width, split ratio, per-tab view state, and other
  implemented view preferences are restored across clean launches.

Floe can remember view settings per folder. When grouping is enabled in Grid
View, full-width group sections share the same authoritative selection as List
View. Ctrl-click, context actions, activation, and switching views continue to
address the exact same files.

## Select files and folders

- Click an item for a single selection.
- Hold `Ctrl` while clicking to add or remove individual items.
- Hold `Shift` to select a range.
- Drag across empty Grid View space for rubber-band selection.
- Press `Ctrl+A` to select everything.
- Press `Ctrl+Shift+I` to invert the current visible selection.
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

Escape dismisses the innermost browser surface first: a context menu, location
editor, unified search, quick preview/inspector, then file selection. Floe then
returns focus to the active List, Grid, or Miller view. Window-parented dialogs
keep their native Cancel/Close behavior.

## Open files and choose applications

- **Open** uses the registered default application through GIO.
- If no default exists, Floe opens the application chooser instead of silently
  failing.
- **Open With…** chooses an application for this launch.
- **Set as Default** is an explicit separate choice; using Open With does not
  silently change the file association.
- **Reset Default** clears the explicit XDG MIME default so the desktop can use
  its recommendations again. Set/reset work runs on a bounded GIO worker and
  reports the actual result.

Applications are launched with native GIO APIs. Desktop applications that accept
local files (`%f`/`%F`) receive a `GFile`; URI handlers (`%u`/`%U`) receive a local
file URI. Floe does not interpolate filenames into shell commands.

### Custom actions

Open **Settings → Applications → File Associations & Custom Actions** to add,
edit, remove, or reorder local external tools. Matching actions appear in the
selected file's right-click menu, and **Run Custom Action…** is searchable from
the Command Palette.

Enter one argument per line. A definition must include at least one exact
placeholder:

- `%f` — first selected path
- `%F` — every selected path as separate arguments
- `%d` — parent folder of the first selection
- `%n` — file name of the first selection
- `%%` — a literal percent sign

You can limit an action to files, folders, one item or multiple items, and MIME
patterns such as `image/*` or `application/pdf`. Floe starts the executable
directly with an `OsString` argv. It does not interpret quoting, `$variables`,
pipes, redirects, `$(commands)`, or any other shell syntax. Custom actions are
ordinary unprivileged external applications: they never inherit administrator
access, vault keys, or remote-resource authority from Floe.

### Bounded administrator access

Administrator browsing is experimental and disabled by default. Open
**Settings → Applications**, enable **Experimental administrator browsing**,
then right-click one local folder or empty folder space and choose **Open as
Administrator…**. The same command is available from the Command Palette.

Floe asks the desktop GVfs `admin` backend to open that exact local folder. The
desktop polkit agent owns any password prompt; Floe never asks for, receives,
stores, or logs the password, and its process remains your normal user. When a
fresh administrator location is not mounted yet, Floe starts one bounded
desktop mount/authorization request and retries the listing after it
succeeds. Cancelling or denying that prompt opens nothing and leaves ordinary
browsing unchanged. If the desktop has no GVfs administrator backend or
authentication agent, Floe reports that limitation and keeps ordinary browsing
unchanged.

The separate view always shows an **Administrator** badge after authorization
succeeds. It supports folder activation, Back/Forward/Parent, Cancel, Retry,
Return to Standard Access, and explicitly confirmed New Folder, Rename,
single-file Copy/Move, Trash, permanent delete, and Unix-mode controls. Existing
destinations are never overwritten; links are not followed; Trash never falls
back to deletion. Return/close is blocked while a mutation is active, and a
failed transfer reports when a partial destination may remain.

Recursive administrator copy, ownership, ACL/xattr/capability/immutable edits,
previews, thumbnails, terminals, archives, Open With, custom actions, and
clipboard operations remain unavailable. The desktop may retain its own
short-lived polkit authorization cache; Floe cannot revoke that desktop-owned
cache. This is a separate reviewed boundary, not a hidden `sudo` shortcut.

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
pane. Floe never silently replaces an existing destination. The conflict
surface compares the incoming and existing item with bounded type, size, and
modified details. Choose **Keep Existing**, **Keep Both**, or **Retry With New
Name** to preserve both versions. For local Copy, Move, and Rename conflicts,
**Replace** is also available after a second destructive confirmation. Floe
rechecks both exact no-follow identities, atomically exchanges the versions,
and privately retains the old destination for Undo. Replacement stops if
either item changed, the private 64-entry backup area is full, or the
filesystem cannot perform an atomic exchange.

**Replace All** appears only for a batch. It applies to later compatible
conflicts in that batch—not other batches—and captures fresh identities for
every item. Cancelling the batch stops pending items. A conflict touching a
Protected Folder pauses for a fresh review instead of inheriting the earlier
authorization.

### Operation progress and history

The Operations Island is non-modal: browsing can continue while a job runs.
It shows determinate progress when totals are known, measured speed and ETA
when meaningful, cancellation, batch pause/resume boundaries, Retry, and
conflict recovery.

Choose **Operation History…** to review the bounded in-session history and use
available safe Undo actions.

Failure toasts show a concise summary and a **Details** button for the bounded
memory-only technical message. Each toast owns its own details, so a newer
notification cannot replace an older toast's explanation. When Floe is not the
active window, completion notifications use generic text and do not include
filenames or paths.

After a successful local Copy, Move, Rename, Create, Duplicate, or Replace,
Floe refreshes the destination if that folder is already visible, selects the
exact resulting item, scrolls it into view, and briefly emphasizes it. Batch
results remain selected together. Floe does not steal keyboard focus or switch
tabs, panes, or folders just to reveal a result. If the result is hidden by the
current filter, inside a collapsed group, missing, or no longer belongs to the
current browser generation, Floe leaves the visible selection alone and shows
general completion feedback instead.

### Recover interrupted operations

Floe writes a small private recovery record before copy, move, rename, or create
work changes the filesystem. If Floe or the computer stops unexpectedly, open
**Main menu → Tools & Safety → Operation Recovery…**. Floe also shows a persistent
Review notification at startup when records need attention.

The Recovery Center shows whether each exact source and destination is currently
present, missing, or inaccessible. Use **Source** or **Destination** to reveal a
recorded path. **Retry** is enabled only for a prior-process copy, move, or rename
whose source still exists and destination is absent. **Mark Resolved** removes
only the journal record; it does not change files. Floe never deletes uncertain
partial output automatically.

If the private journal is corrupt or has unsafe ownership or permissions,
browsing continues but copy, move, rename, and create fail closed. Open Operation
Recovery to review the reason. **Reset Recovery Store** discards only the
unreadable journal after an explicit warning; it never deletes recorded files.

Operation History combines the bounded in-session list with a separate private
durable history for completed local Copy, Move, Rename, Create, Replace, and
supported Floe-owned local Trash work. Durable
records expire 30 days after their last completed Undo or Redo state. Choose
**Undo** for Applied work or **Redo** for Undone work. Floe executes both on a
bounded worker, never overwrites an occupied destination, and rechecks the exact
no-follow identity before an inverse mutation.

Copy and Create Undo use ordinary recoverable Trash. A created directory must
still be empty, so files added later are never removed. Interrupted Undo/Redo
and uncertain partial outcomes appear in Recovery Center for Reveal and
record-only resolution; Floe never deletes uncertain output automatically.
For a local **Move to Trash**, Undo is available only when Floe can prove the
one exact new freedesktop Trash payload and matching `.trashinfo` record created
by that action. Undo restores to the exact original path without overwriting;
Redo must capture a fresh complete Trash receipt. A successful Trash action
that is remote, administrator-owned, ambiguous, malformed, unsafe, or too large
to inspect remains successful but is not advertised as undoable. Permanent
deletion and administrator changes remain outside durable Undo. Safe local
replacement participates in durable Undo/Redo by
atomically swapping the exact current and retained versions; changed or
occupied paths fail closed or enter Recovery Center review. Administrator
dialogs explain that GVfs currently does not return enough exact post-operation
identity evidence to prove a safe fresh-authorized inverse.

## Trash and permanent deletion

- Press `Delete` or choose **Move to Trash** for ordinary recoverable removal.
- Open **Operation History…** to Undo or Redo a supported Floe-owned local
  Trash action while its exact receipt remains valid.
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

Some document thumbnails and previews depend on installed freedesktop provider applications. Floe runs those external helpers only through the required Phase 18L Bubblewrap boundary. If Bubblewrap cannot establish exact target-only/no-network isolation, the provider result is unavailable rather than retried with normal user authority.

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

## Daily-driver organization and details

- Press `Ctrl+N` or choose **Main menu → New Window** for another independent
  browser window. Right-click one folder and choose **Open Folder in New Window**
  to open that exact destination. Closing one window leaves the others running.
  If that window still owns an active copy, move, Trash, checksum, integrity, or
  other file job, Floe keeps it open and tells you to wait or cancel first.
  `Ctrl+Q` uses the same application-wide guard. Idle windows close without
  waiting for a stalled preview, thumbnail, metadata, search, or external-drive
  query. Floe restores a bounded set of normal window workspaces; normal windows
  share bookmarks and preferences, so a change in one appears in the others
  without a stale window reverting it.
- Choose **Natural Name** under **Sort By** when names such as `file2` and `file10` should sort in human numeric order. Ordinary direction, folder placement, grouping and per-folder settings still apply.
- Each bookmark has one **Bookmark options** button. Use it to rename the sidebar label, restore the folder-derived name, move the bookmark up or down, or remove it. Renaming never renames or reconstructs the folder path.
- Choose **Main menu → View & Layout → Sidebar → Collapse or Expand Sidebar** for an icon rail. Floe stores the collapsed state separately from the expanded width, so expanding restores the prior size.
- List column customization now includes **Owner**, **Group**, **Path**, and **Link Target** in addition to the existing image/audio metadata columns. Unknown or unavailable metadata remains blank rather than guessed.
- **Properties** and the Miller **Inspector** expose **Calculate SHA-256…** for one regular file. Calculation begins only after activation and uses the ordinary checksum progress/result UI. A matching hash compares bytes; it does not prove authenticity, safety, ownership or absence of malware.
- **Completion notifications** are enabled by default under **View & Layout**. Floe sends path-free desktop text only for a completed operation that ran at least two seconds and only while its window is not focused. In-app operation results remain authoritative.

Portal Selection Mode displays requester-provided file-type filters and boolean
or list choices. Changing a filter updates visible files in a bounded worker
while folders remain visible for navigation. Floe returns the exact selected
filter and choices. Portal filters are advisory selection aids: if you explicitly
choose or name a valid local file that does not match the active filter, Floe
returns it as required by the XDG portal contract. Floe still does not create
sandbox Document Portal grants.

## Appearance and customization

Open **Main menu → View & Layout** to configure:

- **Appearance**: Native, Glass, Frosted, Minimal, or Compact.
- **Color scheme**: follow the desktop, force Light, or force Dark through
  libadwaita's style manager.
- **Interface font and text scale**: leave the font empty for the system family,
  or enter a validated family name; scale text from 75% to 200%.
- **Reduce motion**: disables Floe-owned CSS transitions and Miller kinetic
  scrolling while respecting the desktop animation policy.
- **Reset appearance**: restores Frosted, system colors/font, 100% text, and
  normal motion.
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
| `Alt+Down` | Recent locations |
| `Ctrl+L` | Edit location |
| `Ctrl+F` | Unified Search in Quick Filter mode |
| `Ctrl+Shift+F` | Unified Search in Search Files mode |
| `Ctrl+H` | Toggle hidden files |
| `Ctrl+T` / `Ctrl+W` | New / close tab |
| `Ctrl+Shift+T` | Reopen closed tab |
| `F3` / `F6` | Toggle Split View / switch active pane |
| `Ctrl+1` / `Ctrl+2` | List / Grid View |
| `Ctrl+A` / `Ctrl+Shift+A` | Select all / clear selection |
| `Ctrl+Shift+I` | Invert the visible selection |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Copy / Cut / Paste |
| `F2` | Rename |
| `Delete` / `Shift+Delete` | Move to Trash / confirm permanent deletion |
| `Space` | Quick Preview |
| `Ctrl+I` | Inspector final column in Miller/Columns View |
| `Alt+Enter` | Properties |
| `Shift+F10` | Keyboard context menu |

Open **Keyboard Shortcuts…** for the authoritative live list because most
shortcuts can be customized.

## Privacy and safety tools

Select one or more items and open **Privacy & Safety** from the right-click menu, the main **Tools & Safety** menu, Properties, or the Command Palette.

- **Inspect Privacy & Safety…** is read-only and local. It explains executable, double-extension, MIME, bidi/control filename signals and supported JPEG/TIFF/PNG/WebP metadata evidence. A result with no reviewed finding is not proof that the item is safe or free of private information.
- **Scan with Local ClamAV…** requires a separately installed and running `clamd`. Floe streams bounded file bytes to its local Unix socket; it does not upload files, bundle signatures, link the ClamAV library, quarantine, or delete. **No known signature reported** means only that the configured local engine reported no known signature for those bytes. Open **Settings → Operations & Safety** to choose **ClamAV maximum file size** (1–16384 MiB, default 1024 MiB) and **ClamAV total scan size** (1–1024 GiB, default 16 GiB). The total is kept at least as large as one configured file. Each report records the limits actually used and offers **Change scan limits…** for future scans.
- **Create Sanitized Copy…** supports JPEG, PNG, and WebP. Floe preserves every source and creates unique ` (sanitized)` siblings. Batch results report unsupported and failed items separately. Floe removes and verifies only documented EXIF/XMP/IPTC/comment/text/time containers; it does not promise anonymity or inspect pixels/steganography.
- The persistent Background Activity row provides **Cancel** for Privacy inspection,
  local ClamAV, and metadata sanitization, then displays **Stopping** until the worker
  acknowledges cancellation. Sanitization and Privacy inspection stop between items;
  already verified sanitized copies remain while every source remains unchanged.
- Completion does not redirect the folder being browsed. Use **View Results** for
  Privacy/ClamAV reports or **Reveal** for a created sanitized copy. Outcomes remain
  visible after switching away from and returning to Floe.

External thumbnail and Preview helpers require an active Bubblewrap boundary. If Bubblewrap is absent or prohibited by the host, those provider-backed results are unavailable and Floe uses its generic fallback; it never silently runs the helper unsandboxed. Normal **Open** and **Open With** remain ordinary desktop application launches. Open Safely was intentionally removed from current scope after its compatibility and interface cost outweighed its daily-use value.

Use **Customize Context Menus…** to hide or show the **Privacy & Safety** right-click submenu. This does not remove the same commands from the main menu or Command Palette.

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

### thumbnail or document preview is missing

Image previews use Floe's built-in reviewed decoders. PDF, office, video, and other system-provided thumbnails can depend on installed freedesktop providers plus usable Bubblewrap user namespaces. Floe falls back to a semantic file icon when no eligible provider is available, the sandbox cannot start, or the provider rejects the file.

### local ClamAV scan is unavailable

Floe does not install or start ClamAV. Install the appropriate ClamAV daemon package for your distribution, update its signatures, and start `clamd`. Floe reviews common local sockets under `/run/clamav`, `/run/clamd.scan`, and `/var/run/clamav`; it does not accept arbitrary remote scanners. Check the distribution service log if the daemon exists but its Unix socket is inaccessible. `clamd` can independently enforce a lower `StreamMaxLength`, `MaxFileSize`, or engine limit. Raising Floe's setting does not override the daemon configuration; an affected result remains **not scanned** and includes the daemon's response.

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
- Open as Administrator is an experimental opt-in read-only view. Privileged
  mutations and an unguarded stable release remain intentionally unavailable.
- Interrupted local copy/move/rename/create recovery is implemented. It is
  conservative restart review, not a transaction log, rollback guarantee, or
  automatic partial-output cleanup.
- Provider sandboxing, Private Mode, Sensitive Folders, encrypted vaults, and
  portable encryption are planned security work, not current claims.
- Permanent delete is not secure erase.

For development and diagnostic commands, see
[Developing Floe](./DEVELOPMENT.md). For exact implementation status, see the
- User-facing Private Mode, Sensitive Folders, encrypted vaults, and portable
  encryption are planned security work, not current claims. Open Safely is
  intentionally deferred and is not a current or recommended feature.
