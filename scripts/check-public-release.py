#!/usr/bin/env python3
"""Dependency-free contracts for Floe's public repository readiness files."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PUBLIC_COMMIT_EMAIL = "37666398+rodriguezcappsec@users.noreply.github.com"
RETIRED_LOCAL_ACCOUNT_PARTS = ("roc", "appsec")


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


def history(errors: list[str]) -> None:
    original_refs = subprocess.run(
        ["git", "for-each-ref", "--format=%(refname)", "refs/original/"],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if original_refs.returncode != 0:
        errors.append("unable to inspect filter-rewrite backup refs")
        return
    if original_refs.stdout.strip():
        errors.append("filter-rewrite backup refs remain reachable")

    identities = subprocess.run(
        ["git", "log", "--all", "--format=%ae%n%ce"],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if identities.returncode != 0:
        errors.append("unable to inspect reachable commit identities")
        return
    unexpected = {
        identity
        for identity in identities.stdout.splitlines()
        if identity and identity != PUBLIC_COMMIT_EMAIL
    }
    if unexpected:
        errors.append(
            "reachable commits contain "
            f"{len(unexpected)} non-public author or committer email value(s)"
        )

    configured_email = subprocess.run(
        ["git", "config", "--local", "--get", "user.email"],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if configured_email.returncode == 0 and configured_email.stdout.strip() not in (
        "",
        PUBLIC_COMMIT_EMAIL,
    ):
        errors.append("repository-local Git email is not the reviewed public noreply identity")

    home = Path.home()
    retired_account = "".join(RETIRED_LOCAL_ACCOUNT_PARTS)
    forbidden_paths = {
        str(Path("/home") / retired_account),
        str(Path("/run/media") / retired_account),
    }
    if str(home) not in ("", "/", "/root"):
        forbidden_paths.update((str(home), str(Path("/run/media") / home.name)))

    commits = subprocess.run(
        ["git", "rev-list", "--all"],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if commits.returncode != 0:
        errors.append("unable to enumerate reachable commits")
        return
    commit_ids = [commit for commit in commits.stdout.splitlines() if commit]

    messages = subprocess.run(
        ["git", "log", "--all", "--format=%B"],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if messages.returncode != 0:
        errors.append("unable to inspect reachable commit messages")
    elif any(path in messages.stdout for path in forbidden_paths):
        errors.append("reachable commit messages contain an account-specific path")

    if forbidden_paths and commit_ids:
        grep_command = ["git", "grep", "-I", "-l"]
        for path in forbidden_paths:
            grep_command.extend(("-e", path))
        grep_command.extend(commit_ids)
        grep_command.append("--")
        history_paths = subprocess.run(
            grep_command,
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        if history_paths.returncode == 0:
            matches = len([line for line in history_paths.stdout.splitlines() if line])
            errors.append(
                f"reachable history contains account-specific paths in {matches} blob view(s)"
            )
        elif history_paths.returncode != 1:
            errors.append("unable to inspect reachable blobs for account-specific paths")

    if not errors:
        refs = subprocess.run(
            ["git", "for-each-ref", "--format=%(refname)"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
        ref_count = len([line for line in refs.stdout.splitlines() if line])
        print(f"public-release-history-ok commits={len(commit_ids)} refs={ref_count}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "check",
        choices=("community", "github", "documentation", "checklist", "history", "all"),
    )
    args = parser.parse_args()
    errors: list[str] = []
    checks = {
        "community": community,
        "github": github,
        "documentation": documentation,
        "checklist": checklist,
        "history": history,
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
