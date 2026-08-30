# Floe Localization and RTL Status

Floe 0.1.0 is English-only. It ships no gettext, Fluent, translation catalogs,
POT file, or translated resources. Phase 21C documents this limit rather than
creating a misleading partial catalog.

## Existing foundations

- Five generic operation-feedback strings use a stable `MessageId` boundary.
- Implemented path feedback uses Unicode first-strong isolation while retaining
  the original `PathBuf` identity.
- Miller navigation follows logical LTR/RTL direction.
- GTK owns standard text direction and input behavior.

Most visible strings remain Rust literals; plural catalogs and a translated
native RTL walkthrough do not exist.

Future localization must extract every visible and accessibility string, use
real plural handling, preserve typed placeholders, isolate untrusted paths,
retain exact non-UTF-8 filesystem identities, test bidi controls and layout
expansion, and install catalogs only with a verified package domain.

Security terms remain distinct: Encrypted Vault means real encrypted storage;
Sensitive Folder means reduced Floe-owned traces; Protected Folder means
accidental-change guardrails; Private Mode means history/cache minimization;
Open Safely requires an active restriction policy; Integrity verified applies
only after verification completes. Only Protected Folder and completed
integrity workflows currently exist.
