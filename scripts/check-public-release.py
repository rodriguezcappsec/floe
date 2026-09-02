#!/usr/bin/env python3
"""Dependency-free contracts for Floe's public repository readiness files."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def load(relative: str, errors: list[str]) -> str:
    path = ROOT / relative
    if not path.is_file():
        errors.append(f"{relative}: required file missing")
        return ""
    try:
        return path.read_text(encoding="utf-8", errors="strict")
    except UnicodeError as error:
        errors.append(f"{relative}: invalid UTF-8: {error}")
        return ""


def require(relative: str, text: str, phrases: tuple[str, ...], errors: list[str]) -> None:
    lower = " ".join(text.lower().split())
    for phrase in phrases:
        if " ".join(phrase.lower().split()) not in lower:
            errors.append(f"{relative}: required contract missing: {phrase!r}")


def forbid(relative: str, text: str, phrases: tuple[str, ...], errors: list[str]) -> None:
    lower = " ".join(text.lower().split())
    for phrase in phrases:
        if " ".join(phrase.lower().split()) in lower:
            errors.append(f"{relative}: prohibited claim remains: {phrase!r}")


def community(errors: list[str]) -> None:
    contributing = load("CONTRIBUTING.md", errors)
    cla = load("CLA.md", errors)
    require(
        "CONTRIBUTING.md",
        contributing,
        (
            "source-available but proprietary",
            "Small bug fixes",
            "large feature",
            "fork",
            "focused branch",
            "cargo fmt --all -- --check",
            "cargo clippy --workspace --all-targets -- -D warnings",
            "CLA.md",
            "submitting a pull request",
            "employer",
            "Sensitive vulnerabilities",
        ),
        errors,
    )
    require(
        "CLA.md",
        cla,
        (
            "retain copyright ownership",
            "perpetual, worldwide, non-exclusive, irrevocable, royalty-free",
            "use,",
            "reproduce, modify, create derivative works",
            "publicly perform",
            "publicly display",
            "sublicense",
            "relicense",
            "proprietary and commercial versions",
            "right to submit",
            "incompatible",
            "employer",
            "give you an ownership interest in Floe",
            "Submitting a pull request",
        ),
        errors,
    )
    forbid("CLA.md", cla, ("reviewed by an attorney", "floe, inc.", "floe llc"), errors)
    if not errors:
        print("public-release-community-ok")


def github(errors: list[str]) -> None:
    ci = load(".github/workflows/ci.yml", errors)
    bug = load(".github/ISSUE_TEMPLATE/bug_report.yml", errors)
    feature = load(".github/ISSUE_TEMPLATE/feature_request.yml", errors)
    pull_request = load(".github/pull_request_template.md", errors)
    codeowners = load(".github/CODEOWNERS", errors)
    require(
        ".github/workflows/ci.yml",
        ci,
        (
            "push:",
            "pull_request:",
            "main",
            "ubuntu-24.04",
            "libgtk-4-dev",
            "libadwaita-1-dev",
            "pkg-config",
            "toolchain: 1.85.0",
            "Swatinem/rust-cache@v2",
        ),
        errors,
    )
    commands = (
        "cargo fmt --all -- --check",
        "cargo check --workspace",
        "cargo clippy --workspace --all-targets -- -D warnings",
        "cargo test --workspace",
    )
    for command in commands:
        count = ci.count(command)
        if count != 1:
            errors.append(f".github/workflows/ci.yml: expected one {command!r}; found {count}")
    forbid(".github/workflows/ci.yml", ci, ("dogtail", "at-spi", "niri", "plasma", "--ignored"), errors)
    require(
        ".github/ISSUE_TEMPLATE/bug_report.yml",
        bug,
        (
            "Floe version or commit",
            "Linux distribution",
            "Desktop environment or compositor",
            "GTK and libadwaita versions",
            "Filesystem or device involved",
            "Reproduction steps",
            "Expected behavior",
            "Actual behavior",
            "clean/private Floe state",
            "Do not upload personal files",
            "sensitive filesystem paths",
        ),
        errors,
    )
    require(
        ".github/ISSUE_TEMPLATE/feature_request.yml",
        feature,
        (
            "Problem to solve",
            "Proposed behavior",
            "Alternatives or workarounds",
            "Affected Floe workflow",
            "Filesystem semantics",
            "Privacy or security",
            "compatibility",
        ),
        errors,
    )
    require(
        ".github/pull_request_template.md",
        pull_request,
        (
            "What changed?",
            "Why?",
            "How was it tested?",
            "Filesystem behavior",
            "Security/privacy",
            "Compatibility",
            "Documentation",
            "CLA.md",
            "constitutes agreement",
        )
        + commands,
        errors,
    )
    require(".github/CODEOWNERS", codeowners, ("@rodriguezcappsec",), errors)
    if not errors:
        print("public-release-github-ok")


def documentation(errors: list[str]) -> None:
    readme = load("README.md", errors)
    security = load("SECURITY.md", errors)
    require(
        "README.md",
        readme,
        (
            "source-available but proprietary",
            "Alpha software",
            "Linux Wayland",
            "Rust 1.85+",
            "CONTRIBUTING.md",
            "CLA.md",
            "Public access does not grant",
            "permission to redistribute",
            "Ordinary **Open** and **Open With**",
            "Protected Folder",
            "Permanent deletion is not secure erase",
            "not a transaction",
            "sensitive paths",
            "Android/MTP",
        ),
        errors,
    )
    require(
        "SECURITY.md",
        security,
        (
            "Private Vulnerability Reporting",
            "credentials",
            "private keys",
            "personal files",
            "sensitive filesystem paths",
            "exploit details",
            "Suspicious-file analysis",
            "Local ClamAV scanning",
            "Privacy Inspector",
            "Permission Audit",
            "Create Sanitized Copy",
            "Bubblewrap",
            "Those applications are not sandboxed by Floe",
            "accidental-change guardrail",
            "Hashes do not establish authenticity",
            "Permanent deletion is not secure erase",
            "not a backup",
            "general security boundary",
        ),
        errors,
    )
    forbid(
        "SECURITY.md",
        security,
        (
            "provider sandboxing, and security scanning are not implemented",
            "Floe is antivirus",
            "files are safe",
        ),
        errors,
    )
    if not errors:
        print("public-release-documentation-ok")


def checklist(errors: list[str]) -> None:
    text = load("docs/PUBLIC_RELEASE_CHECKLIST.md", errors)
    require(
        "docs/PUBLIC_RELEASE_CHECKLIST.md",
        text,
        (
            "v0.1.0-alpha.1",
            "Private Vulnerability Reporting",
            "Keep GitHub Issues enabled",
            "Discussions",
            "branch ruleset or branch protection",
            "Require the **Rust quality gates**",
            "Disable force pushes",
            "repository description",
            "logo and all README links",
            "Git author email",
            "Git history",
            "dependency licenses",
            "advisory",
            "Build the verified native release package",
            "smoke-test",
            "Change repository visibility",
        ),
        errors,
    )
    if not errors:
        print("public-release-checklist-ok")


def worktree_privacy(errors: list[str]) -> None:
    home = str(Path.home())
    if home in ("", "/", "/root"):
        return
    completed = subprocess.run(
        ["git", "grep", "-Il", home, "--", ".", ":!Cargo.lock"],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    files = [line for line in completed.stdout.splitlines() if line]
    if files:
        errors.append("tracked files contain the current account home path: " + ", ".join(files))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("check", choices=("community", "github", "documentation", "checklist", "all"))
    args = parser.parse_args()
    errors: list[str] = []
    checks = {
        "community": community,
        "github": github,
        "documentation": documentation,
        "checklist": checklist,
    }
    selected = checks.values() if args.check == "all" else (checks[args.check],)
    for check in selected:
        before = len(errors)
        check(errors)
        if len(errors) != before:
            break
    if args.check == "all" and not errors:
        worktree_privacy(errors)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    if args.check == "all":
        print("public-release-contracts-ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
