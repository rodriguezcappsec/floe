# Contributing to Floe

Thank you for helping improve Floe. Bug fixes, performance and accessibility
improvements, documentation, tests, UI/UX refinements, security hardening,
platform-compatibility fixes, and carefully scoped new features are welcome.

Floe is source-available but proprietary software. Public access to the source
does not grant permission to redistribute, repackage, sell, or publish a fork or
derivative version. See [LICENSE](./LICENSE) before using the source outside the
official contribution workflow.

## Before starting

Search the existing GitHub issues first. Small bug fixes, documentation fixes,
tests, and obvious regressions can go directly to a focused pull request. Please
open or join an issue before investing substantial work in a large feature,
filesystem-semantic change, new dependency, architecture change, or security or
privacy design. Early discussion avoids incompatible work; it is not intended to
turn small fixes into lengthy proposals.

Sensitive vulnerabilities follow [SECURITY.md](./SECURITY.md), not a public
issue. Never attach personal files, credentials, private keys, or unredacted
filesystem paths to an issue or pull request.

## Development workflow

1. Fork the repository and create a focused branch.
2. Read [AGENTS.md](./AGENTS.md), [Architecture](./docs/ARCHITECTURE.md), and
   [Developing Floe](./docs/DEVELOPMENT.md).
3. Keep filesystem work out of GTK callbacks. Preserve exact `Path`/`PathBuf`
   identity, no-silent-overwrite behavior, bounded background work, and clear
   failure reporting.
4. Add the lowest-layer regression tests that prove the change. Filesystem tests
   must use disposable temporary roots and never real user files or Trash.
5. Run the quality gates below.
6. Open a pull request explaining what changed, why, how it was tested, and any
   filesystem, privacy/security, compatibility, accessibility, or documentation
   effect.
7. Respond to review feedback and keep the branch focused.

## Quality gates

Run from the repository root:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/check-docs.py --strict
git diff --check
```

Graphical GTK, Dogtail/AT-SPI, Niri, Plasma, packaging, or native Wayland gates
may also apply. State exactly what ran and what could not run; a skipped native
gate is not a pass.

## Contribution terms

By intentionally submitting a pull request to Floe, you agree to the
[Floe Contributor License Agreement](./CLA.md). You retain copyright in work you
personally author, while granting the Floe project copyright holder the rights
needed to use the contribution in proprietary and commercial Floe versions.

Only submit work you have the right to contribute. Do not copy code, artwork,
documentation, or other material from incompatibly licensed sources. If an
employer or another party owns the work, obtain permission before submitting it.

Acceptance is not guaranteed. Maintainers may request a narrower design,
additional tests, documentation, or changes needed to preserve Floe's safety,
privacy, performance, accessibility, and Linux compatibility contracts.
