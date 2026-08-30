# Floe Security Policy

Floe is pre-release software. This repository does not yet publish a private
vulnerability-reporting channel or stable supported-version window. Do not send
secrets, credentials, private keys, personal files, or unredacted paths through
a public issue.

Ordinary defects may use a minimal synthetic public issue. No unverified private
contact is promised. Dependency, advisory, license, and release-candidate review
belongs to Phase 21D and is not claimed complete here.

Floe preserves exact Linux paths, avoids shell interpolation, refuses silent
overwrite, and implements bounded integrity and recovery mechanisms. It is not
a sandbox, antivirus, backup system, or general security boundary. Provider
helpers run with ordinary user authority. Administrator browsing is opt-in,
read-only, and never makes Floe root.

Encrypted Vault, Sensitive Folder, Private Mode, Open Safely, Secure Share,
portable encryption, provider sandboxing, and security scanning are not
implemented. Protected Folder is only an accidental-change guardrail. Permanent
deletion is not secure erase. Hashes do not prove authenticity or safety.
Recovery is not a transaction, rollback guarantee, or backup.

See [Privacy and Security](./docs/PRIVACY_SECURITY.md),
[Recovery](./docs/RECOVERY.md), and [Debugging](./docs/DEBUGGING.md).
