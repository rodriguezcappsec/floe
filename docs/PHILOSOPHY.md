# Floe Philosophy

Floe is a spatial file manager for Wayland. It is built around a simple promise:

> Give people powerful control over their files without hiding risk, taking
> ownership away from them, or making the desktop feel fragile.

This page explains why Floe behaves the way it does. For instructions, see the
[User Guide](./USER_GUIDE.md). For the exact implementation status, see the
[Feature Matrix](./FEATURE_MATRIX.md).

## The principles

### Your files remain yours

Floe should not silently overwrite, delete, upload, reinterpret, or reorganize
your data. Recoverable actions should stay convenient. Irreversible actions
should make their scope and consequences clear.

That is why ordinary Delete moves files to Trash without an unnecessary prompt,
while permanent deletion requires explicit confirmation. It is also why
conflicts never silently replace an existing destination.

### Exact identity matters more than convenient text

Linux filenames are bytes, not guaranteed UTF-8 text. Floe may display a lossy
human-readable label, but it keeps the original path as the authority for every
operation.

This is not merely an edge case. A file manager must never act on a different
file because two unusual names happen to look the same on screen.

### Power must remain understandable

Advanced features belong in a daily-driver file manager, but they need names,
descriptions, visible state, and more than one discoverable route. Floe exposes
actions through ordinary menus, context menus, the Command Palette, and keyboard
shortcuts where appropriate.

Power does not mean accepting arbitrary shell text. Custom actions use a direct
executable and structured arguments so filenames cannot become shell syntax.

### The interface remains responsive

Directories, thumbnails, metadata, search, hashing, archives, duplicate scans,
and file transfers can all be slow. Floe keeps them outside GTK callbacks and
uses bounded workers, queues, caches, and cancellation.

The visible result is intentional: long work appears as progress that can be
observed or cancelled instead of freezing the entire file manager.

### Authority stays narrow and visible

Floe does not run its whole interface as root. Password prompts belong to the
desktop authentication service, and elevated authority must never leak into
previews, terminals, external applications, archives, custom actions, or normal
file jobs.

This is why **Open as Administrator…** is currently read-only. It provides
authenticated folder inspection while privileged mutation remains unavailable.
Adding administrator writes safely requires a separate reviewed job boundary,
not a hidden `sudo` shortcut.

### Local first, private by default

Filename search, content search, metadata indexing, checksums, integrity tools,
and duplicate detection are application-owned local operations. Floe does not
upload filenames, hashes, or file contents to perform them.

Features called Private Mode, Sensitive Folder, Encrypted Vault, or Open Safely
must not appear until their stated protection is real. Floe prefers an honest
limitation over a reassuring label with no enforceable mechanism.

### Linux standards before desktop-specific branches

Floe prefers GIO, GLib, XDG, freedesktop standards, portals, and Wayland before
desktop- or compositor-specific code. This keeps ordinary browsing independent
of Niri, Plasma, or any single desktop service.

Special integrations may add value later, but their absence must never make the
core file manager unusable.

### Spatial does not mean unfamiliar

List and Grid views serve familiar workflows. Tabs and split panes support
comparison and transfer. Miller columns expose location and hierarchy as a
spatial relationship.

Floe offers these as complementary views over shared navigation and file state,
not separate applications with inconsistent behavior.

### Visual character remains optional and readable

Native, Glass, Frosted, Minimal, and Compact share the same controls and design
tokens. Transparency is optional, and Glass or Frosted must remain readable when
the compositor provides no blur.

Appearance should express personality without reducing contrast, hiding focus,
or changing how file operations work.

### Security language must describe evidence

A hash can show that bytes changed; it cannot identify who created a file. A
Protected Folder helps prevent mistakes; it does not stop an attacker. Moving a
file to Trash is not secure erasure. A preview helper is not sandboxed unless a
real restriction boundary is active.

Floe names security and privacy features according to what they demonstrably do.

## Why key features behave this way

| Feature or behavior | Why Floe does it this way |
| --- | --- |
| Trash has low friction; permanent delete does not | Recoverable daily actions should be quick, while irreversible data loss requires clear intent. |
| Existing destinations are not silently overwritten | Destination data belongs to the user too; conflicts require an explicit outcome. |
| File operations appear as jobs | Progress, cancellation, conflicts, failure, and recovery should outlive one GTK callback. |
| Quick Preview never intentionally executes active content | Looking at a file must not silently become running it. Unsandboxed helper limitations remain visible. |
| Search and duplicate detection run locally | Filenames, content, and hashes can reveal private information and do not need a remote service. |
| Duplicate results confirm bytes after hashing | A digest narrows candidates, but Floe does not call files identical from a digest match alone. |
| Checksums are not called signatures | Integrity evidence is different from identity or authenticity. |
| Protected Folders are not called encryption | They guard against accidental Floe operations, not other applications or attackers. |
| Administrator browsing is read-only | Temporary elevated authority is kept out of ordinary jobs, external tools, and the rest of the interface. |
| Floe never launches itself as root | Elevating the entire GUI would needlessly expand the damage possible from a defect or untrusted file. |
| Device passwords are handled by the desktop | Floe should not receive, store, log, or reinvent credential handling already owned by GIO and the desktop. |
| Custom actions use direct argument vectors | Users get external-tool integration without filenames being interpreted as shell commands. |
| Window size is remembered but Wayland position is not | Floe owns its useful normal size; the compositor owns placement, workspaces, and monitor policy. |
| Niri, Plasma, remote, and Android integrations are deferred | Standards-based local behavior is the stable foundation; optional integrations should add real value without becoming dependencies. |

## How Floe documents future decisions

When a behavior is surprising, safety-sensitive, privacy-sensitive, or a
deliberate limitation, its documentation should answer four questions:

1. **What does it do?** Use the exact current behavior, not roadmap aspiration.
2. **Why does Floe do it this way?** Connect the decision to a principle above.
3. **What is the tradeoff?** State what becomes slower, unavailable, or more
   deliberate.
4. **What does it not claim?** Separate convenience, integrity, privacy,
   authentication, and security precisely.

These explanations belong near the feature in the User Guide and interface—not
only in developer architecture documents.

## Related documents

- [User Guide](./USER_GUIDE.md)
- [Getting Started](./GETTING_STARTED.md)
- [Accessibility](./ACCESSIBILITY.md)
- [Privacy and Security Architecture](./PRIVACY_SECURITY.md)
- [Privileged Access](./PRIVILEGED_ACCESS.md)
- [Security Policy](../SECURITY.md)
- [Feature Matrix](./FEATURE_MATRIX.md)
- [Roadmap](./ROADMAP.md)
