# Floe Security Policy

Floe is pre-release, source-available proprietary software. Security reports
help protect users, but public source access does not change the rights granted
by [LICENSE](./LICENSE).

## Supported versions

Before the first tagged prerelease, security fixes target the current `main`
branch. After prereleases begin, the project will identify supported versions in
release notes. Older commits and unlisted prereleases should not be assumed to
receive security updates. No response or remediation SLA is promised yet.

## Reporting a vulnerability

Use GitHub's **Private Vulnerability Reporting** for this repository when the
**Report a vulnerability** option is available on the Security tab. That is the
preferred route for a vulnerability, an exploit path, or a report that requires
sensitive technical detail. Maintainers must enable this GitHub setting before
the repository becomes public; see the
[public-release checklist](./docs/PUBLIC_RELEASE_CHECKLIST.md).

Use a normal GitHub issue for a non-sensitive defect that does not expose users
or private data. Do **not** put any of the following in a public issue, pull
request, discussion, screenshot, or log excerpt:

- credentials, access tokens, passwords, or private keys;
- personal files or file contents;
- unredacted usernames, filenames, or sensitive filesystem paths; or
- exploit details that could put users at risk before a fix is available.

Use synthetic reproduction data whenever possible. Review and redact logs before
sharing them; Floe errors can contain paths. If Private Vulnerability Reporting
is not enabled, do not publish sensitive details. A public issue may state only
that a private reporting channel is needed.

## Current implemented safety and inspection features

These features provide narrow evidence or reduce specific risks. None makes a
file, system, or action generally safe.

- **Suspicious-file analysis** reports explainable traits such as executable
  state, launcher/script/AppImage indicators, extension/content-type mismatch,
  double extensions, and Unicode or control-character filename hazards. It is
  not a malware verdict.
- **Local ClamAV scanning** is optional and uses a separately installed,
  configured, running local `clamd` daemon over a reviewed Unix socket. Floe
  streams bounded no-follow regular-file content locally, revalidates identity,
  and distinguishes a reported signature, no known signature, not scanned,
  changed, cancelled, limit, unavailable, and communication outcomes. Floe does
  not upload files, install or update ClamAV, quarantine results, or call an
  unflagged file safe. User-selected Floe limits and `clamd`'s own limits are
  independent.
- **Privacy Inspector** reads bounded, reviewed JPEG/TIFF EXIF and PNG/WebP
  metadata and reports only the evidence it inspected. It is not proof that a
  file contains no other private data.
- **Permission Audit** inspects bounded exact local selections without following
  symbolic links. It reports Unix mode, ownership, queried ACL/xattr/Linux file
  capability and immutable evidence, and mount context. Its conservative fix can
  remove only explicitly reviewed group/other mode bits; it is not an access or
  malware audit.
- **Create Sanitized Copy** supports reviewed JPEG, PNG, and WebP metadata
  removal. It preserves the source, stages with private permissions, revalidates
  identity, verifies the supported removal, and publishes without overwrite. It
  does not promise that every possible identifying pixel or format-specific
  field is removed.
- **External thumbnail and Quick Preview providers** require Bubblewrap. Floe
  launches installed helpers with a reviewed boundary that removes network and
  session-bus access, exposes only the exact input read-only and a private output
  location writable, and enforces process/output/time bounds. If that boundary
  cannot start, the provider fails closed rather than running unsandboxed.

## Important boundaries and non-guarantees

- Ordinary **Open** and **Open With** use normal GIO desktop application launches
  with the signed-in user's authority. Those applications are not sandboxed by
  Floe.
- **Protected Folder** is an accidental-change guardrail requiring additional
  review. It is not encryption, authentication, authorization, or access control
  against other applications or users.
- A matching hash shows that bytes match the compared value under the documented
  race policy. Hashes do not establish authenticity, authorship, freshness,
  trust, or malware safety.
- Permanent deletion is not secure erase. Filesystem snapshots, journal data,
  flash translation layers, backups, and storage behavior may preserve data.
- Recovery and Undo/Redo are conservative, identity-checked aids. They are not a
  backup, durable transaction, universal rollback, or corruption-recovery
  guarantee.
- Floe is not an antivirus product, hostile-content viewer, privilege boundary,
  backup system, or general security boundary. Another process running as the
  same user can often access anything Floe can access.

## Intentionally unavailable security features

Encrypted Vault, Sensitive Folder, user-facing Private Mode, Open Safely, Secure
Share, portable file encryption, quarantine, automatic malware deletion, and a
general sandbox for ordinary application launching are not implemented. Do not
describe planned designs as current protection.

The detailed architecture, threat boundaries, and prohibited claims are in
[Privacy and Security](./docs/PRIVACY_SECURITY.md). Recovery behavior is in
[Recovery](./docs/RECOVERY.md), administrator boundaries are in
[Privileged Access](./docs/PRIVILEGED_ACCESS.md), and safe log collection is in
[Debugging](./docs/DEBUGGING.md).
