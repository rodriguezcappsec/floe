#!/usr/bin/env python3
"""Dependency-free release-document contract for Floe Phase 21C."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
RELEASE_DOCS = tuple(
    Path(name)
    for name in (
        "README.md", "SECURITY.md", "CHANGELOG.md",
        "docs/GETTING_STARTED.md", "docs/INSTALLATION.md", "docs/MIGRATIONS.md",
        "docs/USER_GUIDE.md", "docs/PHILOSOPHY.md", "docs/ADMINISTRATION.md", "docs/ACCESSIBILITY.md",
        "docs/RECOVERY.md", "docs/DEBUGGING.md", "docs/LOCALIZATION.md",
        "docs/PERFORMANCE.md", "docs/RELEASE_MATRIX.md", "docs/ARCHITECTURE.md", "docs/DEVELOPMENT.md",
        "docs/PRIVACY_SECURITY.md", "docs/PRIVILEGED_ACCESS.md",
        "docs/FEATURE_MATRIX.md", "docs/ROADMAP.md",
    )
)
LINK_RE = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")
HTML_LINK_RE = re.compile(r'<a\s+[^>]*href=["\']([^"\']+)', re.I)
HEADING_RE = re.compile(r"^(#{1,6})\s+(.+?)\s*#*\s*$")


def slug(text: str) -> str:
    text = re.sub(r"<[^>]+>", "", text).strip().lower()
    text = re.sub(r"[^\w\- ]", "", text, flags=re.UNICODE).replace(" ", "-")
    return re.sub(r"-+", "-", text)


def report(errors: list[str], path: Path, line: int | None, message: str) -> None:
    errors.append(f"{path}{':' + str(line) if line else ''}: {message}")


def table_checks(path: Path, lines: list[str], errors: list[str]) -> None:
    expected: int | None = None
    ended_table_at: int | None = None
    for number, line in enumerate(lines, 1):
        row = line.startswith("|") and line.endswith("|")
        if row:
            columns = line.count("|") - 1
            if "||" in line:
                report(errors, path, number, "accidental empty/duplicate table column")
            if ended_table_at is not None and number == ended_table_at + 2:
                report(errors, path, number, "table row separated by a blank line")
            if expected is None:
                expected = columns
            elif columns != expected:
                report(errors, path, number, f"table has {columns} columns; expected {expected}")
            ended_table_at = None
        elif not line.strip() and expected is not None:
            ended_table_at = number
            expected = None
        elif line.strip():
            ended_table_at = None
            expected = None


def document_anchors(path: Path, lines: list[str], errors: list[str]) -> set[str]:
    """Return GitHub-style anchors while rejecting ambiguous base slugs."""
    seen: dict[str, int] = {}
    anchors: set[str] = set()
    for number, line in enumerate(lines, 1):
        match = HEADING_RE.match(line)
        if not match:
            continue
        base = slug(match.group(2))
        count = seen.get(base, 0)
        if count:
            report(errors, path, number, f"duplicate heading slug {base!r}")
        seen[base] = count + 1
        anchors.add(base if count == 0 else f"{base}-{count}")
    return anchors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args()
    errors: list[str] = []
    texts: dict[Path, str] = {}
    anchors: dict[Path, set[str]] = {}

    for relative in RELEASE_DOCS:
        path = ROOT / relative
        if not path.is_file():
            report(errors, relative, None, "required release document missing")
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="strict")
        except UnicodeError as error:
            report(errors, relative, None, f"invalid UTF-8: {error}")
            continue
        texts[relative] = text
        lines = text.splitlines()
        anchors[path.resolve()] = document_anchors(relative, lines, errors)
        table_checks(relative, lines, errors)

    for relative, text in texts.items():
        source = ROOT / relative
        targets = [match.group(1) for match in LINK_RE.finditer(text)]
        targets += [match.group(1) for match in HTML_LINK_RE.finditer(text)]
        for raw in targets:
            target = raw.strip().split()[0].strip("<>")
            if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target) or target.startswith("//"):
                continue
            name, _, fragment = target.partition("#")
            destination = (source.parent / unquote(name)).resolve() if name else source.resolve()
            if name and not destination.exists():
                report(errors, relative, None, f"missing local link target {target!r}")
            elif fragment and destination.suffix.lower() == ".md" and unquote(fragment) not in anchors.get(destination, set()):
                report(errors, relative, None, f"missing Markdown fragment {target!r}")

    roadmap = texts.get(Path("docs/ROADMAP.md"), "")
    next_rows = re.findall(r"^\|[^\n]*\|\s*NEXT\s*\|", roadmap, re.M)
    if len(next_rows) != 1:
        report(errors, Path("docs/ROADMAP.md"), None, f"expected exactly one NEXT row; found {len(next_rows)}")

    all_release = "\n".join(texts.values())
    for phrase in (
        "io.github.floe.FileManager",
        "The actual completed phase is **Phase 7A**",
        "Phase 7G Navigation Upgrades is the only `NEXT` phase",
        "recovery journals, and most other security capabilities remain **PLANNED**",
    ):
        if phrase in all_release:
            errors.append(f"stale release claim remains: {phrase}")

    for relative in (Path("README.md"), Path("docs/INSTALLATION.md"), Path("docs/ADMINISTRATION.md")):
        if "io.github.rodriguezcappsec.Floe" not in texts.get(relative, ""):
            report(errors, relative, None, "stable application identity missing")

    readme = texts.get(Path("README.md"), "")
    for phrase in ("Flatpak is not implemented", "not sandboxed", "English-only", "not secure erase", "not a transaction", "sensitive paths", "Android/MTP"):
        if phrase not in readme:
            report(errors, Path("README.md"), None, f"canonical limitation missing: {phrase!r}")

    terms = texts.get(Path("docs/LOCALIZATION.md"), "") + texts.get(Path("SECURITY.md"), "")
    for term in ("Encrypted Vault", "Sensitive Folder", "Protected Folder", "Private Mode", "Open Safely", "Integrity verified"):
        if term not in terms:
            errors.append(f"canonical security term missing: {term}")

    if args.strict:
        for relative, names in {
            Path("README.md"): ("GETTING_STARTED.md", "PHILOSOPHY.md", "ADMINISTRATION.md", "ACCESSIBILITY.md", "RECOVERY.md", "DEBUGGING.md", "LOCALIZATION.md", "SECURITY.md", "CHANGELOG.md"),
            Path("docs/USER_GUIDE.md"): ("GETTING_STARTED.md", "PHILOSOPHY.md", "INSTALLATION.md", "ADMINISTRATION.md", "ACCESSIBILITY.md", "RECOVERY.md", "DEBUGGING.md", "LOCALIZATION.md", "SECURITY.md"),
            Path("docs/PHILOSOPHY.md"): ("USER_GUIDE.md", "FEATURE_MATRIX.md", "PRIVACY_SECURITY.md", "PRIVILEGED_ACCESS.md", "SECURITY.md", "ROADMAP.md"),
        }.items():
            for name in names:
                if name not in texts.get(relative, ""):
                    report(errors, relative, None, f"reciprocal release link missing: {name}")

    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"phase-21c-docs-ok files={len(RELEASE_DOCS)} strict={str(args.strict).lower()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
