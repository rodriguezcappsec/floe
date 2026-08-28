# Floe Feature Matrix

This is Floe's exhaustive capability ledger. It records what the repository
actually implements, what has only a safe foundation, and where remaining work
belongs. `docs/ROADMAP.md` owns sequencing and bounded phase definitions;
`docs/PRIVACY_SECURITY.md` owns the threat model and security claims.

The generic desktop integration baseline is Phase 14; Phase 18A's
documentation-only security architecture, runtime Phases 18T–18Y, and Phase
20A Settings Center are complete. Phase 7G Navigation Upgrades is the only
`NEXT` phase. Every other future capability
remains `PLANNED` or `DEFERRED`.

## Status key

- `COMPLETE`: Implemented in code and covered by repository verification
  appropriate to its scope.
- `PARTIAL`: A useful, safe foundation exists, but the user-facing capability
  is not complete.
- `PLANNED`: Assigned to a future phase; no complete implementation exists.
- `DEFERRED`: Intentionally postponed pending demand, architecture, backend, or
  security review.
- `NOT APPLICABLE`: Floe intentionally rejects the capability or claim.

Phase ranges are dependency destinations, not claims that code exists. The
remaining Phase 6 work is explicitly divided into Trash lifecycle (6N),
transfer semantics (6O), operation control (6P), create/duplicate/links (6Q),
drag and drop (6R), file watching (6S), and browser completeness (6T).

## Filesystem and basic file interaction

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Local directory browsing | `COMPLETE` | 1 | Cancellable background enumeration returns exact `PathBuf` entries to a virtualized UI. |
| Large-directory presentation foundation | `COMPLETE` | 1/6A | Virtualized list/grid and 256-entry main-loop insertion batches avoid eager widget creation. Formal 10k/100k profiling remains Phase 21. |
| Binary-safe Linux paths | `COMPLETE` | 0-6K2 | Core identity uses `Path`, `PathBuf`, `OsStr`, and `OsString`; lossy text is display-only. |
| Non-UTF-8 enumeration, selection, sorting, operations, bookmarks, duplication, and local file URIs | `COMPLETE` | 1-6Q | Focused tests preserve exact raw identity throughout these implemented paths; text-only clipboard commands explicitly reject lossy conversion. |
| Open with registered default application | `COMPLETE` | 2/6I | GIO discovery and launch are asynchronous and use the original path URI. |
| Open when no default is registered | `COMPLETE` | 6I | Normal Open reuses the existing chooser rather than failing at a dead end. |
| One-time Open With | `COMPLETE` | 5D | Compatible GIO applications are shown default-first without changing associations. |
| Explicit Set as Default | `COMPLETE` | 5D | Association changes are separate from one-time Open and report failures. |
| Full association management | `PLANNED` | 19/20 | Needs inspect, set, clear, and user-added external-tool management without shell interpolation. |
| Create folder | `COMPLETE` | 6Q | Validated explicit naming submits a no-overwrite directory request through the bounded create executor. |
| Create empty file | `COMPLETE` | 6Q | Validated explicit naming uses atomic create-new semantics through the bounded create executor. |
| Create New templates | `COMPLETE` | 6Q/12D | Native asynchronous selection starts at XDG Templates when available and copies through the create executor; discovery, categories, and template management remain Phase 12D. || One bounded worker discovers up to 256 no-follow regular XDG template files with exact paths; native empty/error/truncated states and folder management feed no-overwrite creation, and created copies lose execute bits without source changes. |
| Duplicate | `COMPLETE` | 6Q | Multi-selection duplicates run FIFO through bounded batches, preserve symlinks/raw names, and use deterministic no-overwrite `(copy N)` conflict retries. || Multi-selection duplicates run FIFO through bounded batches, preserve symlinks/raw names, and use deterministic no-overwrite suffixes that advance existing `(copy)`/`(copy N)` names without stacking. |
| Inline rename | `PARTIAL` | 4D/12C | A validated modal rename dialog exists; in-place row/grid rename and its QoL contract do not. |
| Rename | `COMPLETE` | 4C/4D | Same-parent exact-name rename uses atomic no-replace semantics and a bounded executor. |
| Symbolic-link preservation during copy/move | `COMPLETE` | 4A/4C | Links are preserved without following their targets. |
| Create symbolic link | `COMPLETE` | 6Q/12E | Explicit validated destination names preserve the exact stored relative target without following it; advanced relative/absolute-link choices remain Phase 12E polish. || Native creation explicitly offers relative or absolute target storage, preserves exact raw path components without canonicalizing/following, validates one destination name, and permits intentionally broken results with truthful guidance. |
| Create hard link | `COMPLETE` | 6Q/12E | Enabled only for one regular non-symlink file; the kernel enforces same-filesystem semantics and Floe reports unsupported/cross-filesystem failures. || Enabled only for one regular non-symlink file; core preflights destination-parent device, reports cross-filesystem limitations, creates without overwrite, and revalidates linked inode identity. |
| Broken-symlink presentation | `PARTIAL` | 1/10C/12E | Entries preserve link identity, but dedicated broken-link status, recovery, and properties are missing. || Entries preserve link identity and Phase 12E creation explicitly explains that relative or absolute links may become broken; dedicated broken-status/recovery presentation remains planned. |
| Reveal symlink target | `COMPLETE` | 6Q/10C/12E | Asynchronous no-follow GIO metadata reads the stored target, resolves relative paths lexically, verifies accessibility, and reveals without executing content. |
| Copy absolute path | `COMPLETE` | 6Q/11A | Copies exact UTF-8 path text and rejects non-UTF-8 paths rather than publishing lossy identity. |
| Copy relative path | `COMPLETE` | 6Q/11A/19 | Uses the current directory as the explicit base and rejects outside-base or non-UTF-8 selections. Repository-relative policy remains later work. |
| Copy filename | `COMPLETE` | 6Q/11A | Copies one or many exact UTF-8 names separated by newlines and rejects lossy conversion. |
| Copy URI | `PARTIAL` | 6Q/11A/14 | Exact local file paths, including non-UTF-8 bytes, become percent-encoded `file://` URIs; non-local location URIs await Phase 14. |
| Hidden-file visibility | `COMPLETE` | 1 | Ctrl+H/action filtering preserves underlying entries; persistent/per-folder policy remains settings work. |
| Refresh current directory | `COMPLETE` | 1/6J | Background context action reloads through the browser worker. |
| External filesystem-change detection | `COMPLETE` | 6S | One active non-recursive GIO monitor coalesces bounded bursts and submits one superseding worker enumeration per accepted batch. |
| Inaccessible directory feedback | `COMPLETE` | 1 | Structured directory errors leave the application usable and surface understandable feedback. |
| Inaccessible individual-file handling | `PARTIAL` | 1-6K2 | Existing operations and thumbnail workers fail safely; richer per-entry status belongs in Inspector/status work. |
| Disappearing-file handling | `COMPLETE` | 1-6S | Workers report missing/stale sources and live reconciliation removes vanished exact identities without reconstructing paths. |
| Root browsing | `COMPLETE` | 6H | The editable absolute location entry can navigate to `/` through the browser worker. |
| Filesystem properties | `COMPLETE` | 10C | On-demand Properties shows containing filesystem type, capacity/free space, read-only state, and enclosing mount facts through the bounded worker. |
| File owner/group editing | `COMPLETE` | 10D | Properties accepts explicit numeric IDs or validated local names; worker-side bounded resolution and normal Unix authority apply without elevating Floe. |
| Permissions editing | `COMPLETE` | 10D | Exact direct/recursive typed jobs preflight the whole bounded no-follow selection, refuse roots/mount crossings, and distinguish cancellation from committed partial failure. |
| Executable-state editing | `COMPLETE` | 10D | Properties can add the user execute bit or remove all execute bits from regular files through the permission job; directories remain unchanged by executable-only edits. |

## File operations, jobs, and conflicts

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| GTK-independent operation/job model | `COMPLETE` | 3 | Strong operation and attempt IDs, progress, commands, events, failures, and legal transitions live outside widgets. |
| Bounded background executors | `COMPLETE` | 4A-10D | Copy, move/rename, Trash, restore, permanent deletion, creation, and Unix permission changes use fixed-capacity application-owned workers. |
| GTK-thread filesystem mutation prohibition | `COMPLETE` | 0-6Q | GTK callbacks submit application commands or asynchronous GIO metadata reads; filesystem mutation stays in core/application workers. |
| Multiple logical operations | `COMPLETE` | 3/6J | Each request has stable logical identity; multi-selection batches serialize requests over bounded workers. |
| Copy files and directories | `COMPLETE` | 4A/4B | Recursive copy has no-follow link policy, chunk cancellation, progress, tracked cleanup, and fail-if-exists behavior. |
| Internal Cut/Copy/Paste | `COMPLETE` | 4B/4D/6J | Application-owned exact-path transfer buffer supports multi-selection batches. |
| Cross-application clipboard | `COMPLETE` | 6O | Publishes bounded `text/uri-list`, GNOME copy/cut, and KDE cut marker formats; asynchronously imports deduplicated local file URIs into exact-path application state. Remote and malformed URIs are rejected. |
| Same-filesystem move | `COMPLETE` | 4C/4D | Linux no-replace rename handles files, directories, and symlinks. |
| Cross-filesystem move | `COMPLETE` | 6O | `EXDEV` uses synchronized hidden sibling staging, source-tree identity revalidation, atomic no-replace publication, and no-follow cleanup. Post-commit cleanup failure is an explicit non-retryable partial result; Phase 18Y now journals interrupted work for conservative restart review. |
| Byte progress | `COMPLETE` | 3/4A | Validated progress is emitted when copy totals are meaningful. |
| Item progress | `COMPLETE` | 6J/6P | Multi-item batches and item-based executors expose explicit item units and completed/total counts. |
| Transfer speed | `COMPLETE` | 6P | Operations Island shows smoothed measured byte rate only after meaningful samples. |
| ETA | `COMPLETE` | 6P | Estimate appears only for determinate byte work with a meaningful rate and disappears on regression, completion, or non-byte progress. |
| Cancellation | `COMPLETE` | 3/4 | Copy is chunk-cancellable, move observes its irreversible boundary, and GIO Trash cancellation is cooperative. |
| Pause command/state vocabulary | `COMPLETE` | 3/6P | Stable batch state distinguishes running, pausing, paused, cancelling, and terminal outcomes. |
| Pause/resume execution | `PARTIAL` | 6P | Serial multi-item batches pause truthfully after the current item and resume FIFO; in-flight syscalls and GIO work are never labelled paused. |
| Retry failed/cancelled work | `COMPLETE` | 5A/5B | Copy, move, rename, and Trash retries keep operation ID and receive a fresh job ID. |
| In-session terminal registry | `COMPLETE` | 5A | Bounded 64-entry terminal state supports recovery without evicting active jobs. |
| User-visible operation history | `PARTIAL` | 6P/18Y | A bounded memory-only dialog exposes terminal work and safe Undo; Phase 18Y provides a separate private interrupted-operation journal; terminal history itself remains memory-only. |
| Clear completed operations | `COMPLETE` | 6P | Clear Completed removes successful entries only and preserves conflict, failed, partial, and cancelled evidence. |
| Operations Island | `COMPLETE` | 4B/5B/5F/6K2 | Non-modal progress, cancel, Retry, and Resolve Conflict use bounded aligned geometry. |
| Completion notification | `PLANNED` | 14/20 | Must respect sensitive notification policy and avoid noisy foreground notifications. |
| Partial failure reporting | `PARTIAL` | 4/6J/6P | Individual jobs fail structurally and batches summarize completed, skipped, failed, and cancelled counts; Phase 18Y now adds conservative restart review. |
| Insufficient-space preflight | `COMPLETE` | 6O | Copy compares planned regular-file bytes with destination `statvfs` user-available bytes before output creation and reports exact required/available values. This is a point-in-time check, not a reservation or completion guarantee. |
| Self-copy/self-nesting rejection | `COMPLETE` | 4A/4C | Core preflight rejects unsafe destination relationships. |
| Destination conflict detection | `COMPLETE` | 4/5E | Existing destinations are distinct conflict outcomes and never overwritten silently. |
| Keep Existing | `COMPLETE` | 5E/5F | A conflict can be acknowledged without submitting another job. |
| Retry With New Name | `COMPLETE` | 5E/5F | Validated raw `OsString` sibling names retain logical operation identity. |
| Keep Both automatic naming | `COMPLETE` | 6P | Bounded deterministic raw-name sibling generation submits fresh atomic no-replace attempts. |
| Replace | `PLANNED` | 6P | Requires explicit overwrite semantics, backup/undo policy, and no silent data loss. |
| Replace All | `PLANNED` | 6P | Depends on reviewed Replace plus scoped apply-to-all decisions. |
| Skip | `PARTIAL` | 5E/5F | Keep Existing is equivalent for one conflict but is not generalized batch Skip policy. |
| Skip All | `COMPLETE` | 6P | Applies only to one stable batch and counts later conflicts as skipped without reopening dialogs. |
| Metadata comparison in conflicts | `PLANNED` | 6P/6T | Phase 6T supplies lazy metadata; Phase 6P uses it without implying equality from weak evidence. |
| Apply-to-all conflict policy | `PARTIAL` | 6P | Batch-scoped Skip All exists. Replace/Replace All remain unavailable until backup/rollback semantics exist. |
| Metadata-complete copy | `PARTIAL` | 4A/6O | Current copy preserves regular-file bytes, directory structure, symlink targets, Unix permission bits, and file/directory access and modification timestamps. Ownership, ACLs, xattrs, security labels, sparse extents, and reflinks remain explicitly unclaimed. |
| Timestamp preservation | `COMPLETE` | 6O | The copy plan reapplies captured access and modification timestamps to regular files and directories after content completion and synchronizes resulting metadata. Symlink metadata is explicitly reported as not preserved. |
| Unix permission-bit preservation | `COMPLETE` | 4A/6O | The core copy plan reapplies source `Permissions` to destination files and directories; richer ACL/xattr ownership remains separate. |
| Extended attributes and ACL preservation | `DEFERRED` | 6O | Implement only after Linux/filesystem support and privacy implications are reviewed. |
| Sparse-file preservation | `DEFERRED` | 6O | Add where practical without corrupting semantic content. |
| Reflink acceleration | `DEFERRED` | 6O | Capability-driven optimization with safe fallback; never change copy semantics. |
| Undo rename | `COMPLETE` | 6P | Completed rename captures destination identity; Undo revalidates it and uses no-overwrite move semantics. |
| Undo move | `COMPLETE` | 6P | Completed same- or cross-filesystem move captures published destination identity; Undo rejects changed/missing objects and occupied original paths. |
| Undo Trash | `PLANNED` | 6N/6P | Depends on standards-correct Trash metadata plus reversible-operation policy. |
| Undo create | `COMPLETE` | 18Y | Completed create captures the no-follow destination identity. Undo uses ordinary recoverable Trash only while identity remains unchanged; created directories must also remain empty, so later user data is never removed. |
| Redo | `DEFERRED` | 6P | Add only after operation-specific undo semantics are proven. |

## Trash and permanent deletion

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Move one item to Trash | `COMPLETE` | 4E/4F | GIO-backed, cancellable request with exact source and affected-parent refresh. |
| Move multiple items to Trash | `COMPLETE` | 6J | Application-owned serial batch avoids silently overflowing worker capacity. |
| Ordinary Trash without confirmation | `COMPLETE` | 4F | Recoverable Trash uses direct action and truthful wording rather needless modal friction. |
| Trash browsing | `COMPLETE` | 6N | First-class local Trash mode enumerates home and mounted-volume freedesktop roots on the bounded browser worker. |
| Restore to original location | `COMPLETE` | 6N | Exact metadata-backed restore uses no-replace move, bounded jobs, and explicit destination-conflict recovery. |
| Restore elsewhere | `DEFERRED` | 6O/20 | Requires destination chooser semantics integrated with the next transfer boundary. |
| Show deletion date and original location | `COMPLETE` | 6N | Values come only from bounded freedesktop `.trashinfo`; unavailable metadata is stated rather than inferred. |
| Empty Trash | `COMPLETE` | 6N | Aggregate safe-focus confirmation submits payload and metadata through Phase 6M with explicit partial-failure semantics. |
| Delete one Trash item permanently | `COMPLETE` | 6N | Selection action reuses Phase 6M and includes the matching metadata record where available. |
| Permanent delete job | `COMPLETE` | 6M | Exact multi-target requests preflight every tree without following symlinks, reject roots and mount boundaries, revalidate identity, and report non-retryable partial failure after commit. |
| Shift+Delete | `COMPLETE` | 6M | Selection-aware shortcut and menu action open an explicit irreversible confirmation with escaped exact target context before submitting the application job. |
| Secure erase claim | `NOT APPLICABLE` | Policy | SSD wear leveling, CoW, snapshots, and remote storage make general secure-erasure claims dishonest. |
| Trash age/size cleanup preferences | `DEFERRED` | 20 | No reviewed portable desktop mechanism currently provides predictable semantics. |

## Drag and drop, creation, and productivity operations

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Intra-folder drag | `COMPLETE` | 6R | Exact selected source identity routes copy/move/link drops into subfolders; same-destination drops fail safely. |
| Drag folders | `COMPLETE` | 6R | Exact lexical destination policy rejects self-nesting before job submission. |
| List to grid and grid to list drag | `COMPLETE` | 6R | Both virtualized views share one exact multi-selection GDK file-list source. |
| Drag to sidebar destination | `COMPLETE` | 6R | Places, bookmarks, and currently navigable mounted devices expose exact directory targets; unavailable devices reject drops. |
| Drag to Trash | `COMPLETE` | 6R | Trash accepts a move action and reuses the bounded application Trash batch. |
| External application to Floe | `COMPLETE` | 6R | GDK/GIO local file lists decode to exact paths; malformed, empty, and non-local payloads are rejected. |
| Floe to external application | `COMPLETE` | 6R | Floe publishes the exact selection as a standard GDK file-list provider with copy/move/link actions. |
| Hover-open folder | `COMPLETE` | 6R | One cancellable 720 ms main-loop timer navigates exact folder targets and clears on leave/drop/shutdown. |
| Drag autoscroll | `COMPLETE` | 6R | Bounded 56 px edge zones adjust the active virtualized view by a clamped 22 px motion step. |
| Drop destination highlighting | `COMPLETE` | 6R | Dashed outline plus action/destination accessible description and status text avoid color-only feedback. |
| Tab reordering by drag | `COMPLETE` | 7B | Stable-ID pointer drag plus Ctrl+Shift+PageUp/PageDown alternatives. |
| Tab detachment | `DEFERRED` | 7F | Optional after tabs, session transfer, and window ownership are stable. |
| Pane-to-pane drag | `COMPLETE` | 7F | Inactive pane resolves the live exact opposite path and reuses copy/move/link job commands. |
| Miller column-to-column drag | `COMPLETE` | 8E | Active/retained exact selections publish standard local file lists; folder and column-background targets resolve exact paths and reuse copy/move/link no-overwrite jobs. Typed hover targets revalidate Miller depth/child identity. |
| Batch rename | `COMPLETE` | 12C | Preview-first bounded Unicode transforms use whole-batch validation, cycle-safe no-replace staging, shared jobs, and exact in-session inverse mapping. |
| Archive compress/extract | `COMPLETE` | 12A-12B | Bounded engine plus native Extract Here/To and Compress workflows reuse shared progress/cancellation and never overwrite existing destinations. |

## Navigation fundamentals

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Current path in application state | `COMPLETE` | 0/1 | `NavigationState` is GTK-independent and authoritative. |
| Back | `COMPLETE` | 1 | Alt+Left/action uses application-owned history. |
| Forward | `COMPLETE` | 1 | Alt+Right/action uses application-owned history. |
| Parent | `COMPLETE` | 1 | Alt+Up/action is disabled at root. |
| Home | `COMPLETE` | 1/6K | Home is a sidebar Place and initial location. |
| Editable location entry | `COMPLETE` | 6H | Pointer/Ctrl+L entry validates explicit absolute local paths off the GTK callback. |
| Failed-location rollback | `COMPLETE` | 6H | Exact prior `NavigationState` is restored after missing, unreadable, or non-directory submissions. |
| Breadcrumbs | `PLANNED` | 7/20 | Resting path is currently one button, not segment-based keyboard-accessible breadcrumbs. |
| Path completion | `PLANNED` | 7/20 | Must be asynchronous, path-safe, and not expose sensitive histories. |
| Persistent/recent location history | `PLANNED` | 7C | Depends on serializable sessions and later privacy-safe history rules. |
| Recent locations surface | `PLANNED` | 7C/14 | Reuse bounded navigation history and suppress persistence in Sensitive/Private modes. |
| CLI path opening | `PLANNED` | 14 | Requires validated command-line/GApplication routing for file and folder targets. |
| Reveal file in folder | `PLANNED` | 7A/14 | Needs navigation plus exact post-load selection and scroll restoration. |
| Restore selection after sorting | `COMPLETE` | 6B/6J | Every selected entry is restored by exact `PathBuf`, including colliding lossy names. |
| Restore selection after refresh | `COMPLETE` | 6S | Manual/job/watcher refresh reconciles exact selected paths and translates bounded rename chains. |
| Restore scroll after refresh | `COMPLETE` | 6S | A stable exact path plus index fallback restores the virtualized view only after 256-entry insertion completes. |
| Back restores prior selection | `PARTIAL` | 7A/7B | Core session history preserves exact multi-selection; Phase 7B wires it to tabs and GTK. |
| Back restores prior scroll | `PARTIAL` | 7A/7B | Core session history preserves exact path/index anchors; Phase 7B wires restoration to the browser. |
| Keyboard-accessible breadcrumbs | `PLANNED` | 7/20 | Required when breadcrumb segments are introduced. |

## Tabs

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Reusable tab/session state model | `COMPLETE` | 7A | Stable-ID bounded core model owns exact path, complete history locations, multi-selection, path/index scroll anchor, sort, grouping, folder placement, view mode, grid size, density, and columns. |
| New/close/switch tab | `COMPLETE` | 7B | Native labelled strip, Ctrl+T/Ctrl+W/Ctrl+Tab, pointer activation, and last-tab window close use bounded stable IDs. |
| Per-tab path and history | `COMPLETE` | 7A/7B | Complete exact location snapshots restore through one shared superseding browser worker. |
| Per-tab view state | `COMPLETE` | 7A-7C | Sort/group/view/grid/density/columns, exact selection, and path/index scroll anchor restore per live tab and across clean restarts. |
| Startup tab/session restore | `COMPLETE` | 7C | Clean shutdown writes versioned bounded live/closed workspace atomically; missing/corrupt/suppressed state falls back to one normal tab. |
| Duplicate tab | `COMPLETE` | 7B | Clones bounded session state beside source, never widget trees or workers. |
| Reorder tabs | `COMPLETE` | 7B | Pointer drag plus Ctrl+Shift+PageUp/PageDown preserve active stable ID. |
| Reopen closed tab / Ctrl+Shift+T | `COMPLETE` | 7C | Bounded 32-entry LIFO reopens with a fresh stable ID and restores complete session state. |
| Close left/right/others | `COMPLETE` | 7C | Tab context commands preserve or deliberately transfer active ownership and feed bounded recently closed state. |
| Foreground/background folder open | `COMPLETE` | 7B | List/grid menu and middle-click background open retain focus; foreground open restores the new tab. |
| Optional tab names | `DEFERRED` | 7C | Add only after default path-derived naming is stable. |
| Pinned tabs | `DEFERRED` | 7C | Requires clear session persistence and close semantics. |
| Session restore | `PLANNED` | 7C | Must be versioned and suppressed in Private/Sensitive modes. |
| Middle-click folder opens tab | `PLANNED` | 7B | Requires pointer parity with an explicit context/command action. |
| Middle-click tab closes tab | `PLANNED` | 7B | Conventional behavior with accessible alternative. |
| Drag tabs | `COMPLETE` | 7B | Stable tab IDs, not indices, own reorder identity. |
| Optional tab detachment | `DEFERRED` | 7F | Needs safe cross-window state transfer and session ownership. |

## Split view

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Split-view state model | `COMPLETE` | 7D | Every tab owns a GTK-independent primary session, optional secondary session, explicit active side, and bounded ratio while retaining stable tab identity. |
| Independent path/history | `COMPLETE` | 7D | Both panes reuse Phase 7A sessions, preserving separate exact paths, histories, selections, scroll anchors, and view policies. |
| Clear active-side indication | `COMPLETE` | 7E | Text states left/right active ownership; CSS only reinforces it. |
| Keyboard side switching | `COMPLETE` | 7E | F6 activates the opposite session through the shared browser pipeline. |
| Copy/move between panes | `COMPLETE` | 7E | Explicit commands resolve the authoritative opposite path and reuse no-overwrite FIFO jobs without replacing staged clipboard state. |
| Open folder in opposite pane | `COMPLETE` | 7E | Exact selected folder updates or creates the inactive session without focus theft. |
| Swap panes | `COMPLETE` | 7E | Swaps session identities and bounded snapshots, not duplicated GTK browser contents. |
| Close pane | `COMPLETE` | 7E | Closes the inactive pane and retains the active session. |
| Persistent split ratio | `COMPLETE` | 7D/7E | Pointer resizing and 5% keyboard steps are clamped to 20–80% and persist through workspace v2. |
| Different view modes per pane | `COMPLETE` | 7D/7E | Each session retains independent view policy and restores it when activated. |
| Active-pane filter/search | `COMPLETE` | 13A-13B | Current-folder filtering and bounded filename search follow the active pane; location changes clear search and same-location refreshes rerun it. |
| Optional synchronized navigation | `DEFERRED` | 7F | Only after independent behavior is reliable and understandable. |
| Drag between panes | `COMPLETE` | 7F | Standard local file-list drops reuse exact no-overwrite copy/move/link requests; action, destination, and commit wording supplement dashed highlighting. |

## Miller / spatial columns

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Column navigation state | `COMPLETE` | 8A | GTK-independent exact parent/selection/child relationships with stable logical depths; no Niri dependency. |
| Virtualized/recycled column UI | `COMPLETE` | 8B | Native horizontally scrolling columns use virtualized list rows; the active column shares the existing browser model while historical results retain at most 16 capped snapshots. |
| Parent context and child relationship | `COMPLETE` | 8A-8B | Core exact direct-child transitions bind visible parent selections and active child columns without reconstructing paths from labels. |
| Left/right directory movement | `COMPLETE` | 8C | Logical parent/child movement reverses for RTL and has exported action alternatives without hard-wiring physical-direction accelerators. |
| Up/down item movement | `COMPLETE` | 8C | Up/Down/Home/End select within bounds and scroll focus visibly; modified selection chords fall through to native GTK behavior. |
| Trackpad horizontal navigation | `COMPLETE` | 8C | Dominant horizontal deltas scroll the outer column surface with clamped adjustment while vertical gestures remain available to column lists. |
| Adjustable column width | `COMPLETE` | 8B | One global 180–520 px width uses explicit Narrower/Wider actions and version-3 preference persistence; no per-path width map. |
| Bounded retained columns | `COMPLETE` | 8A | Core retains at most 16 locations while stable logical depths identify evicted/stale requests. |
| Column context menus/actions | `COMPLETE` | 8D | Active and retained columns emit bounded exact-owner contexts; stale, overflowed, and wrong-parent selections are rejected before existing action/job routing. Pointer and Shift+F10/Menu access share file/background models. |
| Selection preservation | `COMPLETE` | 8A-8C | Exact selected-child state binds retained columns and recycled active lists restore focus-visible bounded keyboard selection. |
| Cross-column drag/drop | `COMPLETE` | 8E | Miller rows/backgrounds, live tab sessions, split panes, Places, bookmarks, and mounted devices share exact destination and bounded hover ownership; two-axis edge scrolling is clamped. |
| Quick Preview final column | `COMPLETE` | 8F | Optional final column exposes exact bounded handoff and hidden/empty/ready/unsupported focus lifecycle. It truthfully loads no content; providers remain Phase 9. |
| Inspector final column | `COMPLETE` | 8F | Optional aggregate-selection hook shares exact bounded lifecycle and explicitly defers metadata providers to Phase 10. |
| Niri-friendly behavior | `PLANNED` | 8/15 | Core mode stays generic; Niri can add optional spatial enhancements later. |
| Plasma-friendly behavior | `PLANNED` | 8/16 | No KDE dependency is required for the base column mode. |
| Optional Vim bindings | `COMPLETE` | 11D | Explicit opt-in reuses the central keybinding/action architecture and never changes normal-user defaults. |

## Views, sorting, grouping, and metadata columns

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Virtualized list/details view | `COMPLETE` | 1/6A | Name, Type, Size, and Modified are bound lazily for visible rows. |
| Virtualized grid/icons view | `COMPLETE` | 6D | Shares one store and `GtkMultiSelection` with list view. |
| Instant list/grid switching | `COMPLETE` | 6D | Ctrl+1/Ctrl+2 and native controls preserve model and exact selection. |
| Persistent global view mode | `COMPLETE` | 6D | Application worker loads/saves atomically. |
| Per-folder view settings | `COMPLETE` | 6T | Opt-in, capped at 256 exact raw-path overrides with explicit global inheritance and private atomic persistence. |
| Grid zoom | `COMPLETE` | 6D | Seven bounded 64-192 pixel sizes with keyboard controls persist. |
| Sidebar density | `COMPLETE` | 6K2 | Compact, Balanced, and Comfortable apply live and persist. |
| Main file-view density | `COMPLETE` | 6T | Compact/Comfortable/Spacious share list/grid widgets, preserve focus, and persist. |
| Compact file view | `DEFERRED` | 20 | Add only if distinct from dense List and useful. |
| Expandable tree view | `DEFERRED` | 20 | Optional; Miller mode is the primary spatial hierarchy investment. |
| No thumbnail-induced layout jumps | `COMPLETE` | 6C-6G | Stable icon slots and fallbacks reserve presentation geometry. |
| Name sorting | `COMPLETE` | 6B | Raw path bytes provide deterministic tie-breakers. |
| Natural name sorting | `PLANNED` | 6T/10B/20 | Needs locale/path-safe policy and stable async ordering. |
| Type sorting | `COMPLETE` | 6B | Uses current coarse textual entry kind. |
| Size sorting | `COMPLETE` | 6B | Unknown values remain last. |
| Modified sorting | `COMPLETE` | 6B | Unknown values remain last. |
| Ascending/descending controls | `COMPLETE` | 6B | Native headings expose arrow, accessible label, and pressed state. |
| Directories first | `COMPLETE` | 6B | Navigable directories stay first in both directions. |
| Configurable directories first/last | `COMPLETE` | 6T | Explicit persisted policy remains independent from sort direction and grouping. |
| MIME sorting | `PLANNED` | 6T/10B | Depends on bounded GIO/shared-mime enrichment. |
| Extension sorting | `COMPLETE` | 6T | Uses original `OsStr` extension identity with exact path tie-breaking. |
| Created/accessed sorting | `PLANNED` | 6T/10B | Filesystem availability varies; unknown values must remain honest. |
| Owner/permissions sorting | `PLANNED` | 6T/10B | Depends on lazy metadata providers. |
| Dimensions/duration/audio sorting | `PLANNED` | 6T/10B/10F | Expensive metadata stays lazy and stable during enrichment. |
| Stable ordering during enrichment | `COMPLETE` | 6T | Lazy metadata responses update bound labels only; deliberate policy actions own resort boundaries. |
| Group by type/date/size/extension | `PARTIAL` | 6T/10B/20 | Type and raw extension grouping have visible list/grid boundaries independent of sorting; dotted directories remain one Folders group. Date and size groups remain planned. |
| Group by tags | `DEFERRED` | 19 | Depends on a real tag model. |
| Collapsible groups | `PLANNED` | 6T/20 | Requires accessible headers and a persistent-state policy. |
| Disable grouping | `COMPLETE` | 6T | None is the default and a persisted explicit grouping choice. |
| Name/Type/Size/Modified columns | `COMPLETE` | 6A | Current four-column hierarchy is compact and virtualized. |
| MIME/Extension/Created/Accessed columns | `COMPLETE` | 6T | Extension uses enumerated identity; MIME, Created, and Accessed load only for bound rows through a fixed-capacity worker. |
| Permissions/Owner/Group/Path columns | `PARTIAL` | 6T/10B | Lazy Unix permissions are implemented; Owner, Group, and Path columns remain planned. |
| Symlink-target column | `PLANNED` | 10B | Must preserve raw target identity and broken-link status. |
| Image/media/audio metadata columns | `PLANNED` | 10F | Dimensions, duration, artist, album, and track depend on reviewed providers. |
| Column visibility selection | `COMPLETE` | 6T | Optional columns use versioned global/per-folder persistence; Name cannot be hidden. |
| Column reorder/resize/autosize | `PARTIAL` | 6T/10B/20 | Pointer resizing plus keyboard/menu narrow/widen actions persist clamped widths; reorder and autosize remain planned. |

## Selection and status surface

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Single and multiple selection | `COMPLETE` | 6J | One shared `GtkMultiSelection` supports list and grid. |
| Ctrl-click toggle | `COMPLETE` | 6J | Native GTK multi-selection behavior. |
| Shift-range | `COMPLETE` | 6J | Native GTK range behavior. |
| Grid rubber-band | `COMPLETE` | 6J | Enabled on `GtkGridView`. |
| Select All | `COMPLETE` | 6J | Ctrl+A and action are exported. |
| Clear selection | `COMPLETE` | 6J | Ctrl+Shift+A, Escape, and action are available in focused view. |
| Invert selection | `PLANNED` | 11A/20 | Central command plus discoverable shortcut/menu. |
| Pattern selection | `PLANNED` | 13A | Reuse current-folder filter matcher without losing exact identity. |
| Same extension/type selection | `PLANNED` | 11A/13A | Use metadata policy, not lossy suffix parsing. |
| Selection survives sorting | `COMPLETE` | 6B/6J | All exact paths are restored. |
| Context-menu selection retention | `COMPLETE` | 6J | Right-click preserves an existing multi-selection or retargets an unselected item. |
| Keyboard-only selection/context access | `COMPLETE` | 5C/6J | Shift+F10/Menu and standard selection keys are supported. |
| Selected item count | `COMPLETE` | 6J | Status policy distinguishes zero, one, and many selected entries. |
| Selected bytes | `COMPLETE` | 6T | Status sums only already-known non-recursive entry sizes and labels the result known. |
| Total item count | `COMPLETE` | 1 | Status reports loading/loaded item counts. |
| Free disk space | `COMPLETE` | 6T | A fixed-capacity GIO worker reports available/total facts for the active local location and mounted local devices; unknown data is omitted. |
| Read-only state | `COMPLETE` | 6T | Current local location and mounted-device status use actual GIO filesystem facts; unknown data is omitted. |
| Loading/error/empty states | `COMPLETE` | 1 | Non-blocking spinner, plain empty state, toast errors, and tracing context exist. |
| Active operation state | `COMPLETE` | 4B/6K2 | Operations Island remains non-modal while browsing continues. |
| Sensitive/private/vault state | `PLANNED` | 18H/18K | Must be explicit and non-color-only after those modes exist. |

## Current-folder filtering and search

Phases 13A–13D share one search row: `Ctrl+F` opens Quick Filter, the
visible mode selector switches to Search Files, and `Ctrl+Shift+F` opens Search
Files directly. Quick Filter narrows loaded entries; Search Files uses the
bounded filename traversal worker. Search Contents is an explicit third mode
that reads bounded local text files. All modes share the optional Phase 13C
advanced predicates and explicit Match Case control.

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Instant text filter | `COMPLETE` | 13A | Case-insensitive current-folder matching runs on already-listed entries; normal order and exact still-visible selection are preserved. |
| Glob filter | `COMPLETE` | 13A | Compile-once filename glob uses Unicode for valid names and deterministic raw-byte matching for non-UTF-8 names. |
| Regex filter | `COMPLETE` | 13A | Compile-once regex runs on the bounded application worker; invalid expressions are reported inline without replacing the current view. |
| Filter match count/clear/Escape | `COMPLETE` | 13A | Header action, Ctrl+F, visible Text/Glob/Regex selector with plain-language popup summaries, hover examples and accessible descriptions, alert feedback, count and clear/Escape behavior share list/grid/Miller backing state. |
| Filename search in current folder/subtree | `COMPLETE` | 13B | Case-insensitive filename-only results stream in batches of 128 through a capacity-1 generation-safe worker with exact path identity. |
| Wider-location search | `PLANNED` | 13B/17 | Depends on generic location architecture and privacy policy. |
| Search cancellation | `COMPLETE` | 13B | Visible Stop cancels by generation, stale events are ignored, and partial results remain with truthful stopped/skipped/truncated feedback. |
| Content search | `COMPLETE` | 13D | Explicit local-only mode applies Phase 13C predicates before no-follow reads; supports UTF-8 and BOM-declared UTF-16, Text/Glob/Regex/Match Case, 64-result batches, line numbers and normalized snippets, truthful binary/encoding/change/limit counters, cancellation and exact-path actions. No remote roots, links, mount crossing, extraction, persistence, indexing, or uploads. |
| Type/extension/MIME filters | `COMPLETE` | 13C | Structured bounded predicates run cheap checks first; GIO guesses MIME from the exact filename without content bytes, and unknown MIME explicitly excludes the candidate. |
| Size/date/owner/hidden filters | `COMPLETE` | 13C | Size/date use enumerated no-follow facts, owner UID is resolved lazily with no-follow metadata, and temporary Include Hidden/Hidden Only never mutates global Show Hidden. |
| Tag filters | `DEFERRED` | 19 | No tag model exists. |
| Glob/regex/case sensitivity | `COMPLETE` | 13C | Text, Glob, and Regex plus explicit Match Case share one query model across Quick Filter and Search Files, including deterministic raw-name fallback. |
| Search history | `COMPLETE` | 13E | At most 32 exact query definitions are deduplicated in memory for this process only; visible Clear Recent and a suppression policy boundary exist. Nothing implicit is persisted, and no Sensitive Folder or Private Mode claim is made before Phase 18. |
| Saved searches | `COMPLETE` | 13E | At most 64 explicitly named searches persist in current version-12 private preferences with raw root bytes, kind/scope/mode and every advanced predicate; corrupt, duplicate, invalid, or over-capacity records are skipped independently. |
| Search sorting/grouping | `PARTIAL` | 13E | Dedicated result lists support deterministic Name, Modified-newest, or Size-largest ordering with exact-path and content-line tie-breakers. Group headings remain deferred rather than inventing a second grouping model. |
| Reveal/search-result context actions | `COMPLETE` | 13B | Dedicated filename/containing-folder rows reuse normal exact-path actions; Reveal navigates to the exact parent and selects the exact result. |
| Optional indexed backend | `COMPLETE` | 13F | Explicit private single-local-root filename/metadata index only. Hidden trees and contents are excluded; exact raw paths, no-follow traversal, same-device/depth/entry/64-MiB bounds, versioned corruption rejection, directory/entry stale checks, `0600` atomic cache, and complete automatic live fallback are verified. No Phase 18 sensitivity/vault claim or content/global/remote/background indexing. |
| Locked-vault search leakage prevention | `PLANNED` | 18J | Locked names/content must not enter global or Floe indexes. |
| Check for Duplicates / duplicate finder | `COMPLETE` | 13G | Explicit local selected files/roots only; bounded same-device no-follow traversal groups exact size, hashes unique identities through reviewed Phase 10E SHA-256, confirms byte-for-byte, revalidates changes, distinguishes hard-link aliases, and counts only independent copies as reclaimable. Capacity-one cancellable worker, memory-only accessible review, exact Reveal, and explicit recoverable Trash handoff are verified. No index dependency, remote/Trash roots, duplicate-result persistence, upload, automatic/permanent deletion, or digest-only proof; Phase 13G3 adds only the validated derived-hash cache recorded below. |

| Duplicate finder workflow maturity | `COMPLETE` | 13G2 | Supersedes the Phase 13G selection-only UI boundary with a native setup window for scanning an explicitly chosen local folder and all subfolders, finding exact copies of one selected regular file within a chosen folder tree, or preserving selected-files/folders scanning. No-selection, one-file, one-folder, and multi-selection defaults are explicit. Reference mode limits hashing to the reference size class and excludes unrelated duplicate groups while preserving exact raw paths, same-device/no-follow traversal, byte confirmation, cancellation, memory-only review, and explicit recoverable Trash. Exact duplicates remain distinct from visually similar media. |
| Duplicate finder cold/warm performance | `COMPLETE` | 13G3 | Size groups receive bounded first/last 64-KiB quick samples before reviewed SHA-256. Hashing is capped at four workers and two reads per filesystem device. A private versioned 200,000-entry/64-MiB derived cache stores exact raw paths, dev/inode/size/mtime/ctime, SHA-256, and bounded recency; exact-fingerprint lookup, file-watcher/subtree invalidation, scan-time mutation rejection, corrupt/insecure/symlink rejection, and atomic `0600` persistence are verified. Warm reuse does not replace quick filtering or final byte comparison, and duplicate results remain memory-only. |

## Thumbnails

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Bounded asynchronous thumbnail boundary | `COMPLETE` | 6C | Capacity-64 worker, stale generation rejection, decode limits, owned RGBA main-thread handoff. |
| Bound-row/cell lazy requests | `COMPLETE` | 6C/6D | Only visible virtualized presentations request work. |
| Persistent freedesktop cache | `COMPLETE` | 6E | Standard normal/large tiers, metadata validation, private writes, and Floe-only cleanup. |
| Raster PNG/JPEG/WebP/GIF/BMP/TIFF/ICO | `COMPLETE` | 6C/6F | Static/first-frame decode with EXIF/TIFF orientation and bounded scaling. |
| Thumbnail HiDPI policy | `PARTIAL` | 6D/20 | Discrete logical edges exist; explicit monitor-scale/provider-output audit remains. |
| Video frame thumbnails | `COMPLETE` | 6L | Uses an installed reviewed freedesktop provider; unavailable/failed providers retain the generic icon. |
| PDF page thumbnails | `COMPLETE` | 6L | Provider output is accepted only as bounded passive PNG; Floe does not intentionally execute document active content. |
| Office/DOCX thumbnails | `COMPLETE` | 6L | Uses installed providers through supervised argv execution; helpers are not sandboxed. |
| Font thumbnails | `COMPLETE` | 6L | Uses installed providers with timeout, cancellation, bounded PNG output, and generic fallback. |
| Text/code thumbnails | `COMPLETE` | 6L | Uses reviewed text MIME policy without a shell or syntax-command interpolation. |
| Embedded audio artwork | `COMPLETE` | 6L | Uses installed audio providers; availability and extraction quality remain provider-dependent. |
| Archive previews | `COMPLETE` | 6L | Uses installed archive providers; helpers run with normal user authority until Phase 18L. |
| Safe SVG thumbnails | `PLANNED` | 18L/9B | Add only with reviewed passive renderer/provider and external-resource denial. |
| AVIF/HEIF/RAW thumbnails | `DEFERRED` | 6L/18L | Add only after provider/decoder and hostile-input review. |
| Remote thumbnail policy | `PLANNED` | 17/18J | Avoid silent full downloads and plaintext cache leakage. |
| Sensitive/vault thumbnail cache policy | `PLANNED` | 18J | Safe default is memory-only, encrypted per-vault cache, or disabled persistence. |
| Sandboxed external thumbnailers | `PLANNED` | 18L | Phase 6L establishes providers; Phase 18L adds the explicit restricted execution boundary. |

## Quick Preview

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Preview provider architecture | `COMPLETE` | 9A | Fixed-capacity typed registry/worker uses exact source identity, explicit limits, cooperative generation cancellation, stale rejection, memory-only cache, deterministic fallback, and bounded GTK draining. No renderer or sandbox claim. |
| Space toggles Quick Preview | `COMPLETE` | 9F | Bare Space is handled only by list, grid, and Miller file views; application-wide editable controls receive ordinary text input, while customized non-typing accelerators remain application scoped. |
| Raster/animated image preview | `COMPLETE` | 9B | Exact no-follow identity and decoder allocation limits produce owned RGBA; animated GIF/WebP is explicitly presented as first-frame-only. |
| Text/Markdown/source/JSON/XML preview | `COMPLETE` | 9B | Bounded UTF-8/BOM UTF-16 source is selectable and inert; binary, HTML, SVG, malformed encodings, scripts and external-resource rendering are rejected. |
| PDF/document preview | `COMPLETE` | 9C | Reviewed installed freedesktop providers return a bounded PNG first-page/document rendition through supervised argv-only execution; helpers retain normal user authority until 18L. |
| Audio/video preview | `COMPLETE` | 9D | Exact local media identity feeds main-thread GTK native playback/seek controls, optional bounded poster, audio fallback, truthful decoder errors, and explicit retired-stream pause/clear. |
| Font/archive preview | `COMPLETE` | 9E | Reviewed bounded PNG font specimens never install; built-in capped ZIP/uncompressed TAR listings preserve raw names, flag unsafe paths, and never extract. |
| Navigate while preview remains open | `COMPLETE` | 9F | Exact target reconciliation cancels stale work and close restores active Miller focus. |
| Image zoom/rotate/fullscreen | `PARTIAL` | 9F | 50–400% zoom/reset and fullscreen are presentation-only; rotation remains deliberately deferred. |
| Media seeking | `COMPLETE` | 9D/9F | Native GTK controls remain responsive and retired streams pause/clear. |
| Preview metadata | `PLANNED` | 9F/10 | Reuse metadata providers rather duplicate parsing. |
| Unsupported/failure state | `COMPLETE` | 9A/9F | Plain, recoverable, accessible, and non-color-only. |
| Read-only safe inspection | `PARTIAL` | 9/18L | Built-ins are passive and helpers are supervised, but provider sandboxing remains 18L. |
| Sandboxed preview providers | `PLANNED` | 18L | Restrict selected file, filesystem, network, temporary storage, time, and resources where enforceable. |
| Vault lock clears preview state | `PLANNED` | 18J | Dependency: vault lifecycle plus privacy-safe in-memory cache invalidation. |

## Inspector, properties, metadata, and checksums

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Inspector foundation / Ctrl+I | `COMPLETE` | 10A | Toggleable, accessible, bounded asynchronous raw-path single/multi-selection facts in the usable final Miller column. |
| General properties | `COMPLETE` | 10C | Native read-only Properties shows exact path display, type counts, MIME, known size, dates, Unix identity, link/dimension/folder facts from bounded Phase 10B providers. |
| Open With properties page | `COMPLETE` | 5D/10C | Single regular-file Properties exposes a deliberate bridge to the existing chooser and explicit default-association action; multi-selection does not imply one association. |
| Owner/group/permissions | `COMPLETE` | 10B-10D | 10B lazily inspects exact Unix UID/GID/mode; 10D adds deliberate direct/recursive editing with local-name resolution, explicit risk acknowledgement, no-follow preflight, cancellation, and truthful partial-failure semantics. |
| Symlink/broken-target properties | `PARTIAL` | 10B-10C | 10B shows the exact raw stored target and bounded no-follow target-entry status; full properties treatment remains 10C. |
| Filesystem/mount information | `COMPLETE` | 10C | Bounded worker queries containing filesystem type/capacity/read-only and enclosing GIO mount name/root; unavailable values stay explicit. |
| Multiple-selection aggregate properties | `COMPLETE` | 10A-10C | Exact selected paths retain aggregate kinds/known bytes/common parent; shared MIME appears only when identical and differing/unknown values are not merged. |
| Recursive folder count/size | `COMPLETE` | 10C | Explicit Properties demand uses cancellable descriptor-relative no-follow traversal capped at 250,000 entries and depth 1,024 with truncation, unreadable, and overflow evidence. |
| Image dimensions/EXIF | `COMPLETE` | 6F/10B/10F | Lazy Inspector, Properties, and opt-in list enrichment expose dimensions plus ten reviewed EXIF presentation fields with no-follow identity checks and explicit malformed/limit states; GPS/privacy findings remain 18O. |
| Media/audio metadata | `COMPLETE` | 10F | Lazy strict parsing exposes bounded duration, title, artist, album, track/disc, genre, year, sample rate, channels, and bitrate facts; no cover-art read, persistent cache, or safety verdict. |
| Exact timestamps and relative dates | `PARTIAL` | 10B/20 | Inspector preserves exact created/modified/accessed `SystemTime` facts and presents local date/time; optional relative presentation remains 20. |
| SHA-256 and SHA-512 hashing | `COMPLETE` | 10E | Exact selected local regular files stream through a capacity-4 worker in 1 MiB chunks with byte progress, cancellation, no-follow opens, and source identity revalidation. |
| MD5 checksum | `COMPLETE` | 10E | Available only as explicitly legacy-labelled compatibility output; it is never presented as modern security or authenticity evidence. |
| Verify expected checksum | `COMPLETE` | 10E | One selected file accepts a strict algorithm-sized hexadecimal digest and reports match or mismatch without authenticity, authorship, freshness, or safety claims. |
| Copy checksum | `COMPLETE` | 10E | The result dialog exposes digest-only clipboard text; filenames and paths are deliberately excluded from that payload. |
| Checksum in Inspector/Properties | `PLANNED` | 20 | Phase 10E provides explicit calculation and results; any Inspector/Properties shortcut must remain on-demand and never hash every file eagerly. |
| Tags/comments | `DEFERRED` | 19 | Requires an interoperable or clearly Floe-owned metadata model. |
| Inspector width persistence | `COMPLETE` | 10A | Independent 180–520 pixel width uses accessible controls and asynchronous version-4 preferences with version-3 migration. |

## Archives and batch productivity

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Archive engine | `COMPLETE` | 12A | Typed exact-path list/extract/compress requests use private staging, atomic no-replace publication, structured progress/cancellation/failures, and a capacity-4 application worker with 16 memory-only outcomes. |
| ZIP and tar family | `COMPLETE` | 12A | ZIP, tar, tar.gz, and tar.xz share bounded path, entry, byte, ratio, duplicate, nesting, link, source-identity, and conflict policy. TAR preserves raw non-UTF-8 member names; ZIP rejects names its reviewed writer cannot represent exactly. |
| 7z | `COMPLETE` | 12A | Reviewed pure-Rust `sevenz-rust` listing/extract/compress uses the same member plan and staging policy; encryption/password support is deliberately disabled. |
| Archive listing/preview | `COMPLETE` | 9E/12A | Engine listing is bounded and validates all members without extraction or execution; Phase 9E remains the lightweight UI preview path. |
| Extract Here/To/into folder | `COMPLETE` | 12B | One exact supported archive resolves a raw-name sibling folder or a chosen local parent plus raw archive stem; engine traversal/link/conflict policy remains authoritative. |
| Compress | `COMPLETE` | 12B | Exact bounded file/folder selection, reviewed format chooser, editable explicit name, destination preview, atomic no-replace publication, and Operations Island cancellation. |
| Archive password handling | `DEFERRED` | 12B | UI truthfully explains the reviewed backend cannot accept passwords and exposes no secret field; no argv, environment, persistence, logs, or helper handoff occurs. |
| Batch rename preview | `COMPLETE` | 12C | Native preview exposes validated old/new mappings with a 128-row presentation cap before one job submission. |
| Prefix/suffix/find-replace/regex | `COMPLETE` | 12C | Pinned Rust regex plus literal transforms, safe-name validation, duplicate detection, and explicit preserve-extension policy. |
| Sequence numbering/padding/case/date | `COMPLETE` | 12C | Deterministic start/padding, case, and stable selected metadata-date templates are shown before apply. |
| Batch rename undo | `COMPLETE` | 12C | One bounded in-session exact inverse mapping is revalidated through the same no-overwrite job; no persistent undo claim. |
| Preferred editor/compare tools | `DEFERRED` | 19 | External tools need safe executable/action configuration without shell interpolation. |

## Sidebar, context actions, commands, keyboard, and terminal

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| XDG Places | `COMPLETE` | 6K | Home plus every distinct existing standard XDG user directory. |
| Exact-path bookmarks | `COMPLETE` | 6K | Versioned raw-path format with bounded async private atomic persistence. |
| Add/remove bookmark | `COMPLETE` | 6K | Current folder can be added; explicit adjacent control removes a bookmark. |
| Reorder/rename bookmark | `PLANNED` | 20 | Preserve raw path identity; rename affects display label only. |
| Bookmark custom icon | `DEFERRED` | 20 | Only if worthwhile after core bookmark editing. |
| Trash and Recent sidebar entries | `PARTIAL` | 6N/14 | Trash is a first-class sidebar location; Recent still needs standards integration and a privacy-safe history policy. |
| Favorites | `PARTIAL` | 6K/19 | Bookmarks cover favorite locations; richer file favorites/tags are not designed. |
| Drives, volumes, mounts, hotplug | `COMPLETE` | 6K | GIO `VolumeMonitor` produces deduplicated application-owned snapshots. |
| Mount/unmount/eject | `COMPLETE` | 6K | Async actions expose busy/unavailable/failure states. |
| Password-protected/encrypted mounts | `COMPLETE` | 6K2 | Window-parented `GtkMountOperation`; desktop owns credentials and Floe is credential-opaque. |
| Safe remove workflow | `COMPLETE` | 6K/18W | Explicit verified copy, exact-mount flush, and revalidated GIO eject/unmount preserves partial states and never claims safe removal before successful removal. |
| Device label and free space | `COMPLETE` | 6K/6T | Device labels come from GIO; mounted local roots receive bounded generation-checked capacity/free/read-only details. |
| Sidebar width persistence/reset | `COMPLETE` | 6K2 | 128-480 px, 320 ms debounce, startup restore, appearance-default reset. |
| Sidebar collapsed mode | `PLANNED` | 20 | Must retain accessible destinations and restore width predictably. |
| Selection-aware file context menu | `COMPLETE` | 5C/6J/10C/12F | List, grid, and Miller share one live selection-aware model with fixed Open/edit/Trash/Delete/Properties actions, default Archives/Batch rename/Links/Terminal/Split groups, optional Copy details/Checksums, and always-reachable customization; Trash retains its purpose-specific model. |
| Directory-background context menu | `COMPLETE` | 6J/12F | Shared list/grid/Miller background model keeps creation, Paste, Select All, Refresh, Edit Location, and customization fixed while Terminal and Split View groups follow the same preference. |
| Expanded context actions | `PARTIAL` | 12-19 | Phase 12F integrates archive, batch rename, links, copy details, checksums, terminal, and split actions. Arbitrary external/plugin commands, per-MIME rules, privacy, and safe-open actions remain with their owning later phases. |
| Avoid giant context-menu wall | `COMPLETE` | 11A/12F | Common actions remain direct, related productivity actions use coherent submenus/sections, and seven bounded optional groups can be shown or hidden without hiding essential recovery/destructive/property/customization access. |
| Context-menu customization | `COMPLETE` | 12F | Native keyboard-accessible editor controls seven reviewed group IDs with deterministic defaults/order, explicit reset/apply, version-8 asynchronous persistence, shared list/grid/Miller updates, and fixed access to customization itself. Reordering, arbitrary commands, plugins, and per-MIME profiles remain deferred. |
| Central command registry | `COMPLETE` | 11A/11C-11E/12F | 69 bounded human-readable commands map to existing GActions; live enabled state remains authoritative, effective accelerators and reviewed placements are centralized, and internal parameterized plumbing is excluded. |
| Command palette / Ctrl+Shift+P | `COMPLETE` | 11B | Native bounded metadata-only search delegates to live GActions, exposes disabled context, keyboard/accessibility semantics, and 16-entry memory-only recents. |
| Customizable shortcuts | `COMPLETE` | 11C/12F | Version-8 preferences support at most 96 command overrides and four bindings per normal/recoverable command, exact conflict feedback, disabling, individual/all reset, legacy migration, and asynchronous persistence. Confirmation-required and irreversible bindings retain reviewed defaults. |
| Optional Vim mode | `COMPLETE` | 11D | Explicit persisted opt-in adds h/j/k/l, g/G, and o only on list/grid/Miller file-view controllers; modifiers, entries, search, spin, text views, and dialogs retain native behavior. |
| Open Terminal Here | `COMPLETE` | 11E | One selected local folder or current local directory launches through a capacity-4 worker using a reviewed absolute executable, no additional arguments, and exact raw working directory; Trash/missing/non-directory/no-provider states fail truthfully. |
| Embedded terminal | `DEFERRED` | 11E | Requires dependency and security architecture review. |
| Security-sensitive shortcut guardrails | `COMPLETE` | 11C/18X | Confirmation-required bindings retain reviewed defaults; all destructive backend dispatches also require fresh exact-scope guardrail authorization. |

## Remote, external-device, and desktop integration

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Generic desktop capability boundary | `COMPLETE` | 14 | App-only stable inventory covers GIO launch, mounts/volumes, XDG folders, portals, notifications, Share availability, theme signals, Secret Service presence, and session-lock reliability without desktop types in core. |
| Desktop integration status | `COMPLETE` | 14 | Refreshable native command/dialog reports available, limited, and unavailable reasons in text; snapshots are bounded, path/content-free, and memory-only. |
| Missing optional desktop-service fallback | `COMPLETE` | 14 | Missing session bus, portal, notifications, Secret Service, or reliable lock signals never disable ordinary local browsing, GIO launching, device monitoring, XDG folders, or appearance fallback. |
| Generic Wayland local behavior | `COMPLETE` | 0-6K2 | Current code uses GTK/GIO/GLib and has no Niri/KDE dependency. |
| Niri detection/IPC/workspace/output | `DEFERRED` | 15 | User-deferred; optional application-layer enhancement with graceful failure. |
| Niri spatial launch/Miller enhancements | `DEFERRED` | 15 | User-deferred; generic Miller remains fully available without Niri. |
| KDE Plasma capability integration | `DEFERRED` | 16 | User-deferred; Plasma remains supported through generic GTK/GIO/Wayland behavior. |
| KWallet credential integration | `DEFERRED` | 16/18 | Only for machine-local secrets that need it; never required for portable passphrase encryption. |
| KDE Connect enhancement | `DEFERRED` | 16/17 | Optional after generic device/location behavior. |
| GIO-backed remote location model | `DEFERRED` | 17 | User-deferred; local `PathBuf` semantics remain unchanged. |
| SFTP/SMB/WebDAV | `DEFERRED` | 17 | User-deferred with the remote location model. |
| Recent servers and saved connections | `DEFERRED` | 17 | User-deferred; no remote location or credential records are added. |
| Remote timeout/retry/offline state | `DEFERRED` | 17 | User-deferred with remote browsing and recovery. |
| Secure remote credential storage | `DEFERRED` | 17/18A | User-deferred; Phase 18A records only a desktop-neutral candidate policy. |
| NFS | `DEFERRED` | 17 | Local mount may already appear as filesystem; dedicated UX only if justified. |
| FTP | `DEFERRED` | 17 | Support only deliberately with security limitations explicit. |
| Remote thumbnails | `DEFERRED` | 17/18J | User-deferred with remote browsing; no remote cache is added. |
| Remote encryption | `DEFERRED` | 17/18B | Requires streaming URI I/O design; never create silent plaintext temp copies. |
| MTP/Android browsing | `DEFERRED` | 17 | User-deferred; existing local GIO device and mount behavior remains. |
| Open as Administrator | `PLANNED` | 14/18 | Requires documented GFile/GVfs `admin://`, polkit, visible authority, safe downgrade, and provider/job tests. |
| Elevate whole Floe process | `NOT APPLICABLE` | Policy | Prohibited; never run the GTK application as root. |
| Capture administrator/mount passwords in Floe | `NOT APPLICABLE` | Policy | Native desktop/polkit/mount operations own authentication. |

## Appearance, settings, and accessibility

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Searchable Settings Center | `COMPLETE` | 20A | Eight plain-language sections, case-insensitive multi-term search, `Ctrl+,`, live existing-preference controls, specialized-editor links, clear empty state, and native accessibility metadata are verified. It adds no duplicate store and cannot disable irreversible confirmations. |
| Header menu information architecture | `COMPLETE` | post-18X | Main menu grouped by task with bounded progressive disclosure; all prior actions shortcuts remain available, while selection-aware context menus retain their customization model. |
| Native/Glass/Frosted/Minimal/Compact presets | `COMPLETE` | 0 | Shared tokens and a radio-style header-menu chooser apply all five presets live. |
| Phosphor interface iconography | `COMPLETE` | post-14 | A pinned local Phosphor Core 2.1.1 Regular subset supplies Floe-owned navigation, action, sidebar, device, status, and detail glyphs; MIT attribution is bundled and no runtime download occurs. |
| File and folder icon styles | `COMPLETE` | post-14 | Floe Color, Phosphor Monochrome, and System Theme switch live from the header menu and persist through version-12 preferences. Plain text, office documents, and PDF remain distinct; each System Theme family has an app-owned fallback. Known extensions outrank synthetic execute bits from exFAT-like mounts; unknown or extensionless executables retain executable artwork. Existing thumbnails still replace generic icons. |
| Semantic icon colors | `COMPLETE` | post-14 | Phosphor entry icons are single-color symbolic SVGs; semantic CSS classes distinguish folder, media, archive, code, document, and generic families while text/type labels preserve non-color meaning. |
| System light/dark semantic colors | `COMPLETE` | 0 | libadwaita semantic colors provide readable light/dark behavior. |
| Optional transparency with readable fallback | `COMPLETE` | 0 | Glass/Frosted remove only the opaque top-level Adwaita background, use distinct semantic alpha layers, and remain readable without claiming or faking compositor blur; verified on dark and bright native Wayland backdrops. |
| User-facing appearance settings | `PARTIAL` | 0/20 | Persistent live preset selection is complete; light/dark/system, custom radius/opacity, and reset UI remain Phase 20. |
| Custom themes/theme tokens | `DEFERRED` | 19/20 | Must extend shared tokens rather fork widget trees. |
| Font family/scale settings | `PLANNED` | 20 | Accessible system-font fallback and no layout breakage. |
| Reduced-motion setting | `PLANNED` | 20 | Honor GTK/system animation policy; custom motion remains restrained. |
| HiDPI/fractional-scaling audit | `PLANNED` | 20 | Verify icons, thumbnails, borders, and focus at actual scale factors. |
| Appearance persistence/migration | `COMPLETE` | 0/6D/6K2 | Version-9 preferences persist one stable preset ID; legacy/invalid values default to Frosted and `FLOE_APPEARANCE` remains a non-mutating launch override. |
| Browsing settings | `PARTIAL` | 20A/20B | Settings Center now controls default view, per-folder memory, Vim navigation, grid size, file density, and sidebar density live. Sort/group/folders-first/hidden/startup/click policy remains audit work. |
| Preview/cache settings | `PARTIAL` | 9F/20A | Settings links explicit memory-only Preview cache clearing. Provider enablement, size limits, persistent cache, and sensitive defaults remain planned. |
| Operation/Trash confirmation settings | `PLANNED` | 6M/20 | Ordinary Trash stays low-friction; irreversible operations remain strongly confirmed. |
| Application preferences | `PARTIAL` | 11E/20A/19 | Settings links the existing reviewed terminal chooser and desktop capability surface. Editor, association, and safe external-tool configuration remain planned. |
| Desktop-specific settings | `PLANNED` | 15/16/20 | Only expose capabilities detected from optional backends. |
| Full keyboard operation | `PARTIAL` | 0-6K2/20 | Core navigation, selection, menus, views, and actions have keyboard routes; future surfaces must complete parity. |
| Logical visible focus | `PARTIAL` | 0-6K2/20 | Native focus and explicit restoration exist in current views/dialogs; comprehensive audit remains. |
| Screen-reader labels | `PARTIAL` | 0-6K2/20 | Current icon buttons, sort state, jobs, and dialogs include labels; Orca audit is pending. |
| Accessible job progress/errors | `PARTIAL` | 4B-6K2/20 | Labels/actions exist; live announcements and full assistive-technology audit remain. |
| High contrast | `PLANNED` | 20 | Validate semantic colors, borders, icons, and non-color state. |
| Font scaling | `PLANNED` | 20 | Test layout and ellipsization with system/user scaling. |
| Localization and RTL readiness | `PLANNED` | 20/21 | User strings, dates, layout direction, path rendering, and translation infrastructure. |
| Non-color-only states | `PARTIAL` | 6B-6K2/20 | Sort arrows, text kinds, operation labels, and device states comply; full product audit remains. |
| Predictable focus after navigation | `PARTIAL` | 6H/20 | Location success/cancel returns focus; tabs/split/preview need a complete hierarchy. |

## Portable encryption and secrets

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Threat model and security architecture | `COMPLETE` | 18A | Assets, adversaries, authority boundaries, non-protections, 16 decisions, dependency admission policy, and a traceable 18B–18AA test plan are documented; no later runtime protection is implied. |
| Portable passphrase encryption | `PLANNED` | 18B-18C | Prefer reviewed interoperable `age` after implementation-time review; streaming, authenticated, cancellable, portable. |
| Portable decryption | `PLANNED` | 18B-18C | Wrong-password, malformed/truncated ciphertext, conflicts, and partial cleanup are mandatory tests. |
| Huge-file streaming | `PLANNED` | 18B | No whole-file buffering; exact paths and progress use job infrastructure. |
| Encryption output conflict handling | `PLANNED` | 18B-18C | Never overwrite silently; partial output is private and safely cleaned/finalized. |
| Encrypt and Trash Original | `PLANNED` | 18C | Only after authenticated output completion; ordinary secure erase is not implied. |
| Recipient/public-key encryption | `PLANNED` | 18D | Reviewed identity representation, fingerprints, multiple recipients where format supports it. |
| Recipient management/import | `PLANNED` | 18D | Public material can be stored; private keys require stricter separate handling. |
| Privacy session / master authorization | `PLANNED` | 18E | Memory-only locked/unlocked/timeout state with manual Lock All and visible status. |
| Secret wrapper and zeroization review | `PLANNED` | 18E | No `Debug`, logs, config, command-line args, or unnecessary cloning. |
| Per-vault credentials | `PLANNED` | 18F-18H | Global privacy session may authorize access but must not silently rewrite vault credentials. |
| Homemade cipher/KDF/MAC/nonce scheme | `NOT APPLICABLE` | Policy | Strictly prohibited. Use established authenticated formats and reviewed maintained libraries. |
| Passwords in config/logs/process arguments | `NOT APPLICABLE` | Policy | Strictly prohibited. |
| Plaintext fallback after encryption failure | `NOT APPLICABLE` | Policy | Strictly prohibited; original remains untouched and failure is explicit. |
| User-facing cipher/KDF knobs | `NOT APPLICABLE` | Policy | Normal settings must not expose dangerous cryptographic choices unnecessarily. |
| “Military-grade” or absolute security claims | `NOT APPLICABLE` | Policy | Marketing language must describe the actual format, mechanism, state, and limitations precisely. |
| Enforced ciphertext/file expiration | `NOT APPLICABLE` | Policy | Anyone retaining ciphertext and a valid key can keep a copy; Floe must not claim otherwise. |

## Encrypted vaults

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Vault backend/format review | `PLANNED` | 18F | Study established interoperable encrypted-storage designs; custom formats incur a high review burden. |
| Envelope key architecture | `PLANNED` | 18F | Password-derived key wraps random vault key so password changes need not rewrite all content. |
| Versioned vault metadata | `PLANNED` | 18F | No silent downgrade; format identifiers and migration rules are explicit. |
| Encrypted filenames/structure | `PLANNED` | 18F-18G | Locked storage must not expose meaningful plaintext names where practical. |
| Recovery-key decision and verification | `PLANNED` | 18F/18H | Threat-model first; recovery material is never logged or silently stored beside ciphertext. |
| Vault virtual filesystem | `PLANNED` | 18G | Review FUSE/overlay alternatives for exact names, random access, crash safety, and no persistent plaintext copies. |
| Vault file/directory operations | `PLANNED` | 18G | Large files, rename, move, truncate, concurrency, symlink/hard-link policy, and disconnect recovery. |
| Create/Add/Unlock/Lock vault UI | `PLANNED` | 18H | Accessible Private sidebar and explicit state, dependent on proven storage/key architecture. |
| Change vault password | `PLANNED` | 18H | Uses envelope architecture; must verify credentials and recovery behavior. |
| Lock All | `PLANNED` | 18E/18H | Clear authorized privacy session and safely lock every eligible vault. |
| Vault auto-lock lifecycle | `PLANNED` | 18I | Timeout, exit, session lock, suspend, open handles, stale mounts, and drive removal. |
| Vault portability | `PLANNED` | 18F-18G | Avoid absolute paths and hidden machine-only secrets; copyable to disk/USB/backup. |
| Privacy-safe vault cache/history | `PLANNED` | 18J | No normal plaintext thumbnails, Recents, search, notifications, or session traces while locked. |
| Password-gated plaintext folder called a vault | `NOT APPLICABLE` | Policy | Prohibited security theater; an Encrypted Vault requires real encrypted storage. |
| Force-unmount that risks corruption | `NOT APPLICABLE` | Policy | Lock UX must explain open-handle delays and preserve data safety. |
| Protection from same-user malware while unlocked | `NOT APPLICABLE` | Policy | Unlocked mounts and external applications can access plaintext; compromised kernels, keyloggers, and screen capture are outside the realistic application boundary. |

## Sensitive folders, Private Mode, and privacy state

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Sensitive Folder marker | `PLANNED` | 18K | Reduces Floe traces/cache only and explicitly does not encrypt the folder. |
| Sensitive thumbnail/preview/history suppression | `PLANNED` | 18J-18K | Cover persistent thumbnails, preview cache, indexing, Recents, notifications, sessions, and paths. |
| Private Floe window | `PLANNED` | 18K | Suppress navigation/search/closed-tab/session/thumbnail/preview histories with clear mode UI. |
| Privacy Lock | `PLANNED` | 18E/18J | Lock vaults, clear sensitive previews/session state, and clear only Floe-owned sensitive clipboard where possible. |
| Privacy & Security Center | `PLANNED` | 18Z | Useful vault/sensitive/integrity/permission state and actions; no fear-based score. |
| Sensitive notification policy | `PLANNED` | 18J | Prefer generic messages where filenames would leak; use desktop lock state only when reliable. |
| Sensitive clipboard policy | `PLANNED` | 18J | Never auto-copy passwords; timeout-clearing is best effort and limitations must be explicit. |
| Privacy-safe navigation/operation/search history | `PLANNED` | 18J | Central trace policy covers every persistent surface. |
| Privacy-safe logging | `PARTIAL` | 0/18A | Current rules avoid contents and verbose paths; secret-type and redaction audit remains Phase 18A/AA. |
| Sensitive Folder described as encryption | `NOT APPLICABLE` | Policy | Prohibited false claim. |
| Private Mode described as cryptographic privacy | `NOT APPLICABLE` | Policy | Prohibited false claim. |
| “Panic Delete” behavior | `NOT APPLICABLE` | Policy | Privacy Lock never destroys user data. |

## Sandboxing and untrusted-file handling

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Sandboxed thumbnail/preview provider policy | `PLANNED` | 18L | Define read-only target, unrelated-filesystem denial, no network, no vaults, temp isolation, time/resource limits. |
| Open Safely | `PLANNED` | 18M | Restricted external launch using reviewed Bubblewrap/Landlock/portal mechanisms and explicit unsupported fallback. |
| Sandbox status indicator | `PLANNED` | 18M/20 | Never claim sandboxing when restriction setup fails; state is not color-only. |
| Download/untrusted-origin indicator | `PLANNED` | 18N | Use only trustworthy platform metadata and explain evidence. |
| Executable/script/desktop/AppImage warning | `PLANNED` | 18N | Combine content/MIME and executable metadata, not extension alone. |
| Double-extension warning | `PLANNED` | 18N | Flag patterns such as `invoice.pdf.sh` without asserting malware. |
| Extension/MIME mismatch | `PLANNED` | 18N | Warning signal only; not proof of malicious intent. |
| Unicode/invisible/misleading filename analysis | `PLANNED` | 18N | Explain bidi/control/confusable risk while respecting legitimate international names. |
| Safe escaped filename display | `PLANNED` | 18N/10 | Inspector exposes why a filename is suspicious. |
| Optional quarantine area | `DEFERRED` | 18N/19 | Only with restore/original-path records and Open Safely; never market as antivirus quarantine. |
| Inspect Read-Only | `PLANNED` | 9/18L | Passive preview path for documents, code, text, images, and archives. |
| Antivirus protection claim | `NOT APPLICABLE` | Policy | Floe has no malware engine and must never claim antivirus protection. |
| Sandbox claim without active sandbox | `NOT APPLICABLE` | Policy | Strictly prohibited. |

## Metadata privacy, permissions, and local sensitive scanning

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Privacy metadata Inspector | `PLANNED` | 18O | Inspect GPS, camera/device/time, author, organization, creator, comments, and embedded metadata by format. |
| Create Sanitized Copy | `PLANNED` | 18P | Preserve original, show before/after findings, safely finalize output, and never overclaim removal. |
| Remove GPS/personal metadata | `PLANNED` | 18P | Format-specific verified providers; failure leaves original untouched. |
| Batch metadata sanitization | `PLANNED` | 18P | Preview, progress, cancellation, partial failure, source preservation by default. |
| Share-time privacy warning | `PLANNED` | 18Q | Risk-based, non-noisy warning with Remove & Share, Share Anyway, Cancel. |
| Secure Share | `PLANNED` | 18Q | Depends on inspection, sanitization, portable encryption, recipient support, and checksums. |
| Unix permission auditor | `PLANNED` | 18R | Explain world-readable/writable and sensitive-key exposure in symbolic and numeric forms. |
| ACL/xattr/capabilities/immutable inspection | `PLANNED` | 18R | Advanced editing remains separate and deliberate. |
| Local sensitive-content scanner | `PLANNED` | 18S | Explicit opt-in, local-only, cancellable heuristic scanning without exposing secret values/logs. |
| Developer secret warnings | `PLANNED` | 18S | Conservative `.env`/SSH/private-key warnings before share or removable transfer; user may proceed. |
| Malware-detection claim for scanner | `NOT APPLICABLE` | Policy | The scanner identifies possible secrets, not malware. |
| Exhaustive metadata-removal claim | `NOT APPLICABLE` | Policy | Only format-specific verified removals may be claimed. |

## Integrity, safe transfer, guardrails, and recovery

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Remember file integrity fingerprint | `COMPLETE` | 18T | Private versioned raw-path records store SHA-256, identity metadata, and the exact target without reconstructing it from display text. |
| Verify saved fingerprint | `COMPLETE` | 18T | Reports match, changed, missing, and stale identity precisely; hash evidence is not authenticity or safety. |
| Portable SHA256SUMS manifest | `COMPLETE` | 18T | Strict GNU-compatible escaping, raw non-UTF-8 names, bounded recursive generation, cancellation, no-overwrite publication, and match/changed/missing/new verification. |
| Signed integrity manifest | `DEFERRED` | 18T | Only after selecting an established signing system; never invent signatures. |
| Integrity monitoring | `COMPLETE` | 18U | Explicit local baseline, private strict storage, bounded same-device no-follow watches, coalescing, rescan-required gaps, cancellation, and understandable diffs. |
| Intrusion-detection claim | `NOT APPLICABLE` | Policy | Integrity monitoring observes selected files; it does not prove compromise or prevent attacks. |
| Copy and Verify | `COMPLETE` | 18V | Optional bounded job revalidates source and destination after ordinary no-overwrite Copy; copied-but-unverified output is retained and reported truthfully on post-copy failure. |
| Copy, Verify, Flush, and Eject | `COMPLETE` | 18W | Explicit staged workflow revalidates the exact removable mount/device, flushes off GTK, and reports safe removal only after successful GIO eject/unmount; real USB lab evidence remains skipped without disposable media. |
| Protected Folder | `COMPLETE` | 18X | Private exact-path accidental-change policy covers destructive source and destination intersections; explicitly not encryption, access control, immutability, or attacker protection. |
| Rich destructive-operation preflight | `COMPLETE` | 6M/18X | Bounded facts and exact action/item/folder/byte/protected/mount context drive safe auto-allow or accessible review; permanent delete retains its separate irreversible confirmation. |
| Operation journal | `COMPLETE` | 18Y | Copy, move, rename, and create workers persist a bounded versioned raw-path record before mutation under private atomic XDG state storage; success or proven-absent failed output removes it. No passwords, keys, file contents, hashes, or display-derived paths are stored. |
| Crash/interrupted-operation recovery | `COMPLETE` | 18Y | Startup Recovery Center reports current source/destination state and offers reveal, conservative retry only for intact source plus absent destination, and record-only resolution. Corrupt/insecure storage blocks journaled mutations until explicit reset; uncertain output is never deleted automatically. |
| Snapshot integration | `DEFERRED` | 18X/19 | Capability-driven Btrfs/ZFS/external-tool integration only; Floe is not a filesystem administrator by default. |
| Integrity hash described as signature | `NOT APPLICABLE` | Policy | Hashes provide integrity evidence, not signer authenticity. |
| Protected Folder described as attacker security | `NOT APPLICABLE` | Policy | It prevents mistakes only. |

## Extensibility and developer features

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Custom context actions | `PLANNED` | 19 | Capability-aware, failure-contained actions with safe executable/argument representation. |
| Custom commands/scripts | `DEFERRED` | 19 | Never shell-interpolate filenames; security permissions are required before general scripting. |
| File-type actions and external tools | `PLANNED` | 19 | User-added actions are separate from MIME default application choices. |
| Templates | `PARTIAL` | 6Q/12D/19 | Safe native template selection and bounded no-overwrite creation are implemented; discovery, management, and broader extensibility remain planned. || Safe bounded XDG discovery, native selection/management, non-executable no-overwrite creation, and post-refresh naming are complete; user-defined categories and broader extensibility remain Phase 19. |
| Share actions | `PLANNED` | 14/19 | Phase 14 reports generic Share availability conservatively but transmits nothing; an explicit action remains future standards/portal work. |
| Plugin runtime | `DEFERRED` | 19 | Only after demonstrated demand and capability/isolation design; no automatic vault access. |
| Git repository/status badges | `DEFERRED` | 19 | Must remain cheap when unused and respect ignored/private content. |
| Repository terminal/copy-relative-path | `DEFERRED` | 11E/19 | Depends on terminal integration and repository detection. |

## Performance, packaging, and release

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| Bounded queues/caches and no unbounded threads | `COMPLETE` | 1-6K2 | Current directory, operation, preference, bookmark, and thumbnail work is bounded. |
| Startup profiling | `PLANNED` | 21 | Measure cold/warm startup on supported Wayland environments. |
| 10k/100k directory benchmark | `PLANNED` | 21 | Verify enumeration, insertion, sorting, selection, memory, and interaction latency. |
| Thumbnail-scroll/cache stress | `PLANNED` | 21 | Include huge image/system-thumbnailer folders and worker saturation. |
| Copy/move/hash/encryption throughput | `PLANNED` | 21 | Measure without weakening cancellation, integrity, or responsiveness. |
| Vault/metadata/scanner/integrity performance | `PLANNED` | 21 | Required only after owning features exist. |
| Desktop file/app metadata/icons | `PLANNED` | 21 | Release-quality application integration and MIME declarations. |
| `.age` association | `DEFERRED` | 21 | Only after portable encryption format is selected and implemented. |
| Flatpak/Arch/AUR packaging | `DEFERRED` | 21 | Select after dependency, sandbox, portal, and vault/FUSE requirements are known. |
| Configuration/cache migration | `PLANNED` | 20/21 | Preserve current preferences/bookmarks/cache and future versioned formats across upgrades. |
| Vault-format compatibility policy | `PLANNED` | 18F/21 | Security-sensitive migration and backward-compatibility rules. |
| Dependency/security audit | `PLANNED` | 18AA/21 | Hostile files, crypto, parsers, sandbox assumptions, secrets, caches, lifecycle, and recovery. |
| Release documentation/accessibility/readiness | `PLANNED` | 21 | Native Niri, Plasma, and generic Wayland testing with truthful limitations. |

## Quality-of-life checklist

These small behaviors are acceptance requirements, not optional polish.

| Capability | Status | Phase | Notes |
| --- | --- | --- | --- |
| F2 starts rename | `COMPLETE` | 4D | Current dialog opens from shortcut/action. |
| Rename selects name without extension | `PLANNED` | 12C/20 | Default selection excludes extension; an easy full-name selection remains available. |
| Escape cancels rename / Enter confirms | `COMPLETE` | 4D | Focused validated dialog follows conventional response behavior. |
| Selected state survives rename | `COMPLETE` | 6S | Exact watcher rename pairs translate the selected identity before refreshed model installation. |
| Renamed item remains visible | `COMPLETE` | 6S | Exact watcher rename pairs translate selected and anchored identities before the refreshed model is installed. |
| New Folder enters rename | `COMPLETE` | 6Q/12D | Creation succeeds first; the exact new directory is then selected for naming. || The exact destination remains memory-only, is selected after asynchronous refresh, and enters the existing rename workflow only after it appears. |
| Selection survives refresh | `COMPLETE` | 6S | Exact-path reconciliation preserves surviving items and drops disappeared identities. |
| Scroll survives refresh | `COMPLETE` | 6S | Stable anchor identity is preferred with a clamped prior-index fallback when the anchor disappears. |
| Back restores item and scroll | `PARTIAL` | 7A/7B | Complete exact selection and path/index anchor are stored per history entry; runtime tab/browser restoration is Phase 7B. |
| Human-readable list size | `COMPLETE` | 6A | Decimal formatting supports values through exabytes. |
| Exact bytes in details | `PLANNED` | 10C | Inspector/Properties show both exact and human-readable values. |
| Relative dates plus exact timestamp | `PLANNED` | 10B/20 | Exact value remains available in Inspector. |
| Async folder-size calculation | `PLANNED` | 10C | Cancellable bounded traversal. |
| Sensible tooltips | `PARTIAL` | 0-6K2/20 | Current icon controls and ellipsized names use tooltips; full audit remains. |
| Minimal confirmation friction | `COMPLETE` | 4F | Recoverable Move to Trash is not needlessly confirmed. |
| Strong irreversible confirmation | `COMPLETE` | 6M/18X | Permanent delete retains its irreversible confirmation and adds exact action/scope/risk guardrail review where required. |
| No focus stealing after jobs | `PARTIAL` | 4B-6K2/20 | Operations Island is non-modal; full notification/recovery audit remains. |
| Reveal completed destination | `PLANNED` | 6Q/11A | Reuse the Phase 6Q navigation/reveal action and preserve focus. |
| Configurable single/double click | `PLANNED` | 20 | Default remains conventional double-click/Enter until preference exists. |
| Sidebar width persists | `COMPLETE` | 6K2 | Debounced complete-state preference and reset are verified across launches. |
| Split ratio persists | `PLANNED` | 7D/20 | Depends on split state. |
| Consistent focus after navigation | `PARTIAL` | 6H/20 | Existing location flow restores view focus; future surfaces require audit. |
| Predictable Ctrl+L | `COMPLETE` | 6H | Seeds/selects current display path and validates explicit absolute submission. |
| Predictable Escape hierarchy | `PARTIAL` | 4D/6H/6J/20 | Rename/location/selection paths exist; preview, filter, dialogs, tabs, and panes need one hierarchy. |
| Browsing during operations | `COMPLETE` | 4B | Non-modal Operations Island and background workers keep the browser available. |
| Non-blocking error feedback | `COMPLETE` | 1-6K2 | Toasts/dialog recovery preserve application usability. |
| Detailed errors available | `PARTIAL` | 1-6K2/20 | Structured failures and logs exist; a user-facing details surface is incomplete. |
| Hidden/filter/search mode obvious | `COMPLETE` | 1/13A/13B | Hidden toggle, Text/Glob/Regex filter mode, and distinct filename-search mode expose visible scope, Search, Stop, Close, progress, and result state. |
| Active pane/tab obvious | `PLANNED` | 7B/7E | Non-color-only state and predictable focus. |
| Private/vault/sandbox status obvious | `PLANNED` | 18H/18K/18M | Text/icon/accessibility state, never color alone. |
| Watcher storms coalesced | `COMPLETE` | 6S | One 140 ms cancellable timer deduplicates paths and caps 16,384 events, 4,096 paths, and 1,024 rename pairs before conservative overflow reconciliation. |
| Context menus selection-aware | `COMPLETE` | 6J | Existing multi-selection is preserved when appropriate. |
| Upgrades preserve settings | `PARTIAL` | 6D/6K2/20 | View preference parser is backward compatible; full migration framework remains. |
| Shortcut discoverability | `COMPLETE` | 0-6K2/11C | Header menu, Ctrl+?, and command palette open one searchable native dialog listing every registered command, category, description, availability, effective bindings, and custom/default state. |
| Password reveal/hide and Caps Lock feedback | `PLANNED` | 18C/18H/20 | Use native accessible password widgets where platform support exists. |
| Password confirmation on vault creation/change | `PLANNED` | 18H | Prevent mistyped unrecoverable credentials without weakening key design. |
| Conservative wrong-password errors | `PLANNED` | 18B/18H | Do not leak unnecessary oracle detail; remain useful to legitimate users. |
| Encryption/decryption failure preserves original | `PLANNED` | 18B-18C | Atomic authenticated finalization and no silent overwrite. |
| Vault lock clears plaintext preview | `PLANNED` | 18J | Includes in-memory thumbnail/preview/provider state. |
| Security warnings explain evidence | `PLANNED` | 18N/18R/20 | Plain-language reason plus technical detail; no fear-based wording. |

## Architectural dependency ledger

| Dependent capability | Status | Phase | Required foundation |
| --- | --- | --- | --- |
| Cross-filesystem move | `COMPLETE` | 6O | Synchronized staged copy, atomic no-replace publication, source identity revalidation, and conservative partial cleanup exist. Phase 18Y now provides conservative interrupted-operation restart review. |
| Undo | `PLANNED` | 6P | Requires explicit operation-specific reversible semantics and current-state revalidation. |
| Tabs/session restore | `COMPLETE` | 7A-7C | Versioned bounded raw-path workspace restores live/closed state through private atomic storage; explicit Private/Sensitive policy suppresses owned traces. |
| Split view | `PLANNED` | 7D-7F | Reusable navigation sessions and explicit active-pane ownership. |
| Miller columns | `COMPLETE` | 8A-8F | Exact model, virtualized columns, keyboard/trackpad, actions, cross-surface drag/drop, and truthful final-column Preview/Inspector handoff are verified. Provider content remains Phases 9/10. |
| Quick Preview | `PLANNED` | 9A-9F | Existing thumbnails plus cancellable provider boundary designed for Phase 18L sandboxing. |
| Inspector | `COMPLETE` | 10A-10F | Shared bounded lazy metadata providers; no eager whole-directory enrichment, persistent metadata cache, privacy finding, or authenticity claim. |
| Command palette | `COMPLETE` | 11A-11B | Central command registry and metadata-only palette delegate execution and eligibility to existing GActions. |
| Archives | `COMPLETE` | 12A-12B | Engine/job lifecycle, native workflows, conflict handling, cancellation, and traversal/bomb/link defenses are complete for reviewed local formats. |
| Search/indexing and duplicate discovery | `PARTIAL` | 13A-13G | Current-folder filtering and bounded non-indexed filename subtree search are complete; advanced/content/saved/indexed search and duplicate discovery remain later phases. |
| Niri/Plasma integrations | `DEFERRED` | 15-16 | User-deferred; generic desktop capability boundary remains complete in Phase 14. |
| Remote locations | `DEFERRED` | 17 | User-deferred, including Android/MTP; local browsing and mounted-device support remain. |
| Portable encryption | `PLANNED` | 18B-18C | Phase 18A threat model plus existing progress/cancellation/conflict jobs. |
| Encrypt and Trash Original | `PLANNED` | 18C | Authenticated encryption completion plus existing Trash job. |
| Recipient encryption | `PLANNED` | 18D | Reviewed public identity representation and portable format support. |
| Vault UI | `PLANNED` | 18H | Proven vault storage/key architecture in 18F-18G. |
| Vault auto-lock | `PLANNED` | 18I | Safe mount/unmount and open-handle lifecycle. |
| Private preview/search | `PLANNED` | 18J | Privacy-safe cache/history/index architecture and vault lifecycle. |
| Sandboxed Preview | `PLANNED` | 18L | Explicit provider process boundary and enforceable restriction policy. |
| Open Safely | `PLANNED` | 18M | Defined sandbox policy and truthful failure/unsupported behavior. |
| Secure Share | `PLANNED` | 18Q | Metadata inspection, sanitization, encryption, recipients, checksum, safe output. |
| Integrity monitoring | `COMPLETE` | 18U | Explicit private baselines plus bounded coalesced same-device watcher and rescan-required gaps. |
| Copy and Verify | `COMPLETE` | 18V | Existing safe copy engine plus source/destination revalidation and reviewed streaming SHA-256. |
| Copy, Verify and Eject | `COMPLETE` | 18W | Verified Copy plus exact-mount flush and relationship-aware GIO eject/unmount infrastructure. |
| Crash recovery | `PLANNED` | 18Y | Privacy-aware operation journal and explicit partial-destination semantics. |
