# Privileged access design

Status: experimental read-only Phase 14B implementation complete; privileged
mutations and removal of the experimental guard remain gated.

## Implemented Phase 14B boundary

Floe exposes **Open as Administrator…** only after the user enables
**Experimental administrator browsing** in Settings. The action accepts the
current local folder or one explicitly selected local directory. A private
constructor starts from its exact absolute `PathBuf`, round-trips a GIO `file`
URI, changes only the scheme through GLib/GIO URI builders, and rejects hosts,
credentials, query, fragment, foreign schemes, arbitrary administrator URIs,
relative paths, round-trip drift, and unexpected provider children.

The application-owned service creates and uses administrator GFiles on the
owning GLib main context. It requests no-follow metadata in 128-entry pages,
retains at most 4,096 entries, has a 120-second authorization deadline and
30-second page deadline, uses GIO cancellables, and emits generation-bound typed
events. Only the current request's first successful page enters visible
Administrator state. The separate virtualized view provides folder-only
Back/Forward/Parent navigation, Cancel, Retry, and Return to Standard Access.

This implementation is deliberately read-only. No privileged resource can
reach local mutation jobs, preview/thumbnail code, terminals, Open With,
archives, external/custom actions, clipboard operations, plugins, or the main
path-only navigation model. Native Wayland smoke verified both window actions,
open/cancel/return lifecycle, liveness, clean quit, and UID 1000 before and after
activation. Niri-specific and separate Plasma lab gates were unavailable, so
the feature remains experimental and the unguarded stable-release gate stays
closed.

## Goal and non-goals

`Open as Administrator…` should let a normal Floe process browse a local directory
through the desktop's authenticated GIO/GVfs `admin://` backend. GVfs delegates
authorization to polkit and the session authentication agent. Floe remains the
calling user's GTK application throughout.

This is capability-scoped access, not a root mode:

- Floe must never run the whole GTK application as root, relaunch itself through
  `sudo`/`pkexec`, or start an elevated helper shell.
- No command is assembled from a filename, URI, or display label. GIO APIs receive
  typed `GFile` resources; there is no shell or path interpolation.
- Floe never asks for, stores, forwards, or logs a password. Authentication UI and
  credential handling belong to the session polkit agent and GVfs.
- Privileged access does not elevate external applications, terminals, plugins,
  previews, thumbnailers, or context-menu tools. Initial privileged views use
  generic icons and metadata only.
- This design does not promise that every distribution supplies the GVfs admin
  backend or a working polkit agent. Unsupported systems retain normal browsing.

An encrypted or password-protected volume is a different workflow. Mount/Unlock
must use GIO's `GMountOperation` with a system-integrated `GtkMountOperation` (or
equivalent portal/desktop implementation), so Floe code does not collect the
passphrase. After mounting, normal permissions apply; `admin://` is offered only
for a separate local filesystem permission failure.

## Threat model and invariants

Filenames, symlinks, desktop-provided metadata, URIs, backend errors, and mounted
media are untrusted. The design must resist privilege confused-deputy mistakes,
lossy path reconstruction, URI substitution, time-of-check/time-of-use races,
unexpected link traversal, accidental overwrite, partial operations, stale
authorization, and a missing or maliciously failing desktop backend.

The following invariants are release blockers:

1. A privileged resource carries both its exact local `PathBuf` identity and the
   exact canonical GFile URI identity returned or derived through reviewed GIO/GLib
   URI APIs. The local path preserves non-UTF-8 bytes. Neither identity is rebuilt
   from lossy display text.
2. Constructors for privileged identities are private to the privileged-access
   service. They accept only absolute local paths, create a `gio::File::for_path`,
   parse its encoded `file://` URI, and use a GLib URI builder to change only the
   scheme to `admin`. They reject a host, user-info, query, fragment, unexpected
   scheme, or a URI/path round-trip mismatch. String concatenation is not allowed.
3. Children come from the `GFileEnumerator` child handle plus the exact
   `FileInfo::name` bytes. The display name is presentation-only. Tests must prove
   that colliding lossy names remain distinct.
4. Authority is explicit in every location, entry, selection, command, job, and
   event. A privileged resource can never fall through to a `PathBuf`-only local
   executor. Mixed standard/privileged selections are rejected until a deliberate
   cross-authority transfer design exists.
5. The service accepts only local `admin` resources that it created. It never
   accepts arbitrary user-provided administrator URIs, remote hosts, embedded
   credentials, or another URI scheme.
6. Normal-level logs contain operation/job IDs and classified outcomes, not
   credentials, file contents, full paths, or administrator URIs. Debug path
   logging remains opt-in and redacted.
7. No public application action, D-Bus activation parameter, plugin API, or
   external-tool hook accepts a privileged URI or privileged identity. The only
   authority entry point is an in-process command built from the exact current
   local resource after explicit user activation.

## Why the current browser cannot expose the action

The current implementation is correctly local-path-only:

- `floe_core::NavigationState`, `DirectoryListing`, and `DirectoryEntry` own
  `PathBuf` values.
- `BrowserWorker::request` accepts a `PathBuf` and calls the Rust/std-backed core
  directory enumerator.
- `BrowserController` navigation history, selection restoration, bookmarks, and
  action dispatch all assume local paths.
- copy, move, rename, and Trash requests in `ApplicationState` accept local
  `PathBuf` targets and dispatch to local executors (Trash converts that path with
  `gio::File::for_path`).

An `admin://` URI is not a Linux path. Putting it in a `PathBuf` would make it an
ordinary malformed local path and discard its GFile authority; converting the
URI back to a display path would discard provenance and can corrupt non-UTF-8
names. Reusing the same local `PathBuf` would simply run the existing unprivileged
enumerator. Elevating the whole process would violate process isolation and make
every parser, thumbnailer, plugin, and UI code path privileged. Therefore the
action must not be exposed until the resource and provider interfaces below are
implemented.

## Interfaces that unblock implementation

The GTK-independent navigation history should become generic over an opaque
location identity (or receive a path-independent replacement). Parent locations
must be supplied by the active provider rather than calculated with `Path::parent`.
The application layer then owns these non-GTK types:

```rust
enum ResourceId {
    Local(PathBuf),
    PrivilegedLocal {
        local_path: PathBuf,
        admin_uri: Box<str>,
    },
}

struct BrowserLocation {
    id: ResourceId,
    parent: Option<ResourceId>,
    access: AccessLevel,
}

enum AccessLevel {
    Standard,
    Administrator,
}
```

The real fields should be private and validated; the sketch is not permission to
construct these values freely. `floe-core` remains free of GTK, GIO, GVfs, and
polkit types. Path-independent entry metadata should remain in core, while an
application-layer browser entry pairs that metadata with `ResourceId`. Local
enumeration adapts the existing `DirectoryEntry`; privileged enumeration supplies
the same metadata shape without pretending its identifier is only a path.

Introduce an application-owned provider boundary:

```text
GTK action
  -> BrowserCommand / PrivilegedCommand
  -> BrowserProvider router
       -> LocalProvider (existing bounded BrowserWorker)
       -> PrivilegedAccessService (GIO async on its owning GLib context)
  -> typed BrowserEvent / JobEvent
  -> BrowserController presentation
```

Minimum commands are capability query, open privileged location, enumerate page,
cancel request, leave privileged access, and submit privileged mutation. Every
request carries a generation/request ID and `ResourceId`; every result echoes
both. Stale results are discarded exactly as local browser generations are now.
Enumeration uses bounded pages and lazy metadata, not one unbounded result.

`PrivilegedAccessService` owns all administrator `GFile`, `GFileEnumerator`, and
`gio::Cancellable` objects on the GLib context where they were created. The
separate device service owns any `GMountOperation`. GTK callbacks only submit
commands and render events. The services must not block the main loop, and GIO
objects must not be sent to the existing `std::thread` browser worker unless
their bindings explicitly guarantee that transfer.

Privileged mutations use a separate typed request such as
`PrivilegedMutationRequest { operation_id, sources: Vec<PrivilegedResourceId>,
destination, conflict_policy }`. `ApplicationState` registers it with the central
job manager, but a privileged GIO executor performs it. Existing local
`CopyRequest`, `MoveRequest`, `RenameRequest`, and `TrashRequest` are never reused
by stripping the URI. Progress, conflict, cancellation, retry eligibility,
partial failure, and affected-resource refresh return through structured job
events. A retry reauthorizes/revalidates resources and receives a fresh `JobId`;
it never assumes a cached administrator capability is still valid.

## Capability discovery and fallback

At startup, querying `gio::Vfs::default().supported_uri_schemes()` may record
whether `admin` is advertised; this check must not prompt. The action is eligible
only for an absolute local directory and an advertised backend. Actual access is
tested only after the user activates `Open as Administrator…`, by asynchronously
querying/enumerating the service-created `admin://` GFile with a cancellable.
One authorization attempt may be active per view; repeated activation focuses its
existing progress surface instead of spawning authentication-prompt storms.
Scheme support grants no mutation capability: each write operation additionally
checks the corresponding GFile capability and remains disabled until reviewed.

Backend outcomes are classified rather than flattened:

- unsupported scheme/backend: explain that this system does not provide GVfs
  administrator access;
- no authentication agent or authorization denied: keep the standard view and
  say that access was not granted;
- backend unavailable/disconnected: keep the standard view and offer Retry;
- target missing/not a directory: keep the standard view and identify the target
  generically;
- cancellation/timeout: keep or restore the exact prior standard location;
- success: enter privileged state only after the first authenticated listing is
  accepted for the active request generation.

Denial, authentication failure, and timeout never auto-retry because that could
create a prompt loop. Retry is an explicit user action with a new request ID.

There is no `sudo`, `pkexec`, shell, whole-process root, silent fallback to local
I/O, or distribution-specific bypass. Help text may name the missing GVfs admin
backend/polkit agent category, but must not guess an installation command.

The browser access state machine is explicit:

```text
Standard
  -> Authorizing(request ID, return location, cancellable)
  -> Privileged(location, return location)
  -> Leaving
  -> Standard
```

Unsupported, denied, failed, cancelled, timed-out, or stale authorization results
transition from `Authorizing` to the captured `Standard` return location. Only a
successful first listing for the current request can enter `Privileged`. The
state contains no password, polkit cookie, or reusable authorization token; any
desktop-side authorization cache remains opaque to Floe.

## Visible and reversible privileged state

Permission-denied local listings may offer `Open as Administrator…` in the error
surface. The same action may appear in local directory, Place, Bookmark, or
mounted-local-root context menus. It is unavailable for files, remote/non-local
roots, aggregate devices, or arbitrary typed URIs.

After successful authentication, the header and location surface show a
persistent, high-contrast `Administrator` badge with an accessible label and
tooltip. Color is not the only signal. The badge menu provides:

- `Return to Standard Access`, which cancels privileged requests, discards all
  privileged selections and operation affordances, and restores the exact
  pre-elevation local location;
- `Close Privileged View`, which closes that navigation context (or returns to the
  standard location while Floe has only one view).

Back/forward history retains access level per entry and never crosses into an
administrator location without displaying the badge. Opening a new tab, split,
or window starts standard unless the user explicitly requests administrator
access there. Closing a privileged view cancels its outstanding requests and
releases all service-owned GFiles/cancellables. Floe states clearly that the
desktop may cache polkit authorization and that closing a view cannot revoke a
session agent's cache.

Leaving a view must not conceal privileged work. If a mutation is active,
Return/Close offers `Keep Operation Running` and `Request Cancellation` before
the view downgrades. Either choice leaves an `Administrator operation` badge on
the Operations Island until the terminal event. Cancellation-requested is not
shown as cancelled, and closing the navigation context never discards the job
record or partial-failure report.

Location-bar text is never authoritative identity. Editing while privileged
either submits an explicitly validated local path through the privileged service
while retaining the badge, or first returns to standard access; this behavior
must be unambiguous in the UI. Copy path exposes the original local `PathBuf`
lossily for display/clipboard only and never feeds it back into an operation.

## Timeouts, cancellation, and lifecycle

- Scheme capability lookup is local and nonprompting. If it cannot finish
  immediately, the action stays unavailable rather than blocking startup.
- Open/authentication has a 120-second deadline and a visible Cancel action. On
  expiry Floe calls `gio::Cancellable::cancel`, returns to standard access, and
  reports `TimedOut`; it does not infer denial or success.
- Enumeration fetches bounded pages. Each page has a 30-second no-response
  watchdog and cancellation; a new navigation generation cancels the old one.
- Long mutations have no unsafe wall-clock kill. A 30-second no-progress watchdog
  changes the Operations Island to `Still waiting…` with Continue Waiting and
  Cancel. Cancellation remains `requested` until GIO reports a terminal outcome.
- Timeout/cancellation cannot claim rollback. For rename, move, delete, mount, or
  other commit-like calls, Floe refreshes every affected parent and reports
  completed, cancelled, failed, or partially completed exact-item outcomes.
- Application shutdown cancels requests, stops accepting privileged jobs, drains
  terminal events for a bounded interval, then releases service resources. It
  never leaves the UI alive as root because the UI was never elevated.

## Symlinks, conflicts, and destructive operations

Privileged enumeration requests `standard::type`, `standard::name`,
`standard::display-name`, `standard::is-symlink`, and required metadata with
`GFileQueryInfoFlags::NOFOLLOW_SYMLINKS`. The exact name bytes and child GFile are
authoritative. Activating a symlinked directory is an explicit navigation step;
the resulting location preserves both link origin and resolved destination for
the breadcrumb/inspector. Recursive copy, move, and deletion never follow
symlinks by default and operate on the link itself.

All mutation defaults are fail-if-exists. GIO overwrite flags are absent unless a
future, separately reviewed conflict decision explicitly authorizes one exact
target. Before commit, the service re-queries source and destination identity and
metadata; a change returns a conflict instead of silently continuing. Conflict
dialogs show operation, source, destination, access level, and whether the target
is a link. There is no global `apply to all` for privileged work in the initial
release, and retry never broadens scope.

Initial rollout is read-only. Each write class (create/rename, copy/move, Trash,
permanent deletion, permissions/ownership) requires its own threat review and
rollout gate. Privileged Trash must not fall back to permanent deletion when the
backend does not support Trash. Permanent deletion is labelled `Delete
Permanently`, never `secure erase`; it requires an irreversible confirmation with
the exact target count, never follows links, reports per-item partial failures,
and is implemented as a dedicated cancellable job. External launch, preview,
thumbnail generation, archive extraction, and plugin actions remain disabled for
privileged resources until independently reviewed.

## Test gates

The action must not be exposed until automated tests prove all of the following:

1. Two local non-UTF-8 `PathBuf` names with the same lossy label keep distinct raw
   paths and distinct canonical GFile URI identities through enumeration,
   selection, sorting, navigation, cancellation, and refresh.
2. Identity constructors reject relative paths, remote hosts, credentials,
   fragments, queries, injected schemes, arbitrary `admin://` input, and
   file/URI round-trip mismatch. Display labels cannot create operation targets.
3. Provider routing sends local IDs only to the local worker and privileged IDs
   only to the privileged service. Mixed-authority selections and stale generation
   events are rejected without I/O.
4. A fake privileged service covers success, denial, no agent, unsupported GVfs,
   disconnect, cancellation, timeout, stale response, and partial failure without
   touching real user data or requiring root.
5. Navigation preserves authority in back/forward/parent state; downgrade and
   close cancel outstanding work, clear selections, restore the prior exact local
   path, and never hide the badge while privileged content remains visible.
6. Symlink fixtures prove no-follow enumeration and recursive mutation policy.
   Conflict fixtures prove fail-if-exists, fresh revalidation, no silent overwrite,
   and no broadened retry.
7. GTK interaction tests prove keyboard/pointer parity, accessible administrator
   state, Cancel, Return to Standard Access, and correct disabled actions.
8. Operation/job tests prove bounded dispatch, structured progress, cancellation
   requested versus confirmed, no unsafe automatic retry, affected-parent refresh,
   exact per-item partial failures, and redacted logs.
9. Mount tests prove encrypted-device authentication is delegated to a supplied
   `GMountOperation` UI and no Floe state, tracing field, or persistence record
   receives a passphrase.

## Rollout gates

1. Land the dual-identity resource/provider refactor with no administrator action;
   all existing local browsing and operation tests must remain green.
2. Land capability detection and an internal fake-service flow. Unsupported or
   denied systems must remain fully usable and must never invoke a shell/helper.
3. Complete native Wayland smoke tests on both Niri/generic mode and KDE Plasma,
   using an isolated disposable root-owned fixture and confirming process UID
   never changes. If either environment is unavailable, record that fact but do
   not waive the missing stable-release gate.
4. Security review must verify URI construction, non-UTF-8 identity, polkit/GVfs
   delegation, redacted logs, no-follow behavior, timeouts, cancellation, and that
   external parsers/tools receive no administrator capability.
5. Enable read-only `Open as Administrator…` behind an experimental setting.
   Collect classified backend outcomes, never paths or URIs, and verify graceful
   fallback across at least one system with and one without `admin` support.
6. Remove the experimental guard only after native accessibility, lifecycle,
   denial/cancellation, and repeated open/downgrade/close tests pass without leaked
   privileged state. Mutations stay gated independently.

The current working action remains behind the experimental setting because
Niri-specific, separate Plasma, root-owned-fixture, and unguarded repeated-
lifecycle stable-release gates are not all available. Do not remove that guard
or add privileged mutations until their applicable gates pass; a disabled or
classified fallback remains preferable to an unsafe or misleading path.
