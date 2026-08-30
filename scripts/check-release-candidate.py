#!/usr/bin/env python3
"""Deterministic Floe release-candidate policy checks."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = ROOT / "packaging" / "release-policy.json"
MATRIX_PATH = ROOT / "docs" / "RELEASE_MATRIX.md"
SPDX_ID_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9.+-]*")
SPDX_OPERATORS = {"AND", "OR", "WITH"}


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def metadata() -> dict:
    completed = subprocess.run(
        ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(completed.stdout)


def resolved_packages(meta: dict) -> list[dict]:
    resolved = {node["id"] for node in meta["resolve"]["nodes"]}
    return sorted(
        (package for package in meta["packages"] if package["id"] in resolved),
        key=lambda package: (package["name"], package["version"], package["id"]),
    )


def license_ids(expression: str) -> set[str]:
    return {
        token
        for token in SPDX_ID_RE.findall(expression)
        if token not in SPDX_OPERATORS and not token.endswith("-exception")
    }


def check_dependencies() -> int:
    policy = load_json(POLICY_PATH)
    allowed = set(policy["allowed_license_ids"])
    packages = resolved_packages(metadata())
    errors: list[str] = []
    registry = "registry+https://github.com/rust-lang/crates.io-index"

    for package in packages:
        name_version = f'{package["name"]} {package["version"]}'
        expression = package.get("license")
        if not expression:
            errors.append(f"{name_version}: missing license expression")
        else:
            unknown = license_ids(expression) - allowed
            if unknown:
                errors.append(
                    f"{name_version}: license outside policy: {', '.join(sorted(unknown))}"
                )

        source = package.get("source")
        manifest = Path(package["manifest_path"]).resolve()
        if source is None:
            try:
                manifest.relative_to(ROOT)
            except ValueError:
                errors.append(f"{name_version}: path dependency outside repository")
        elif source != registry:
            errors.append(f"{name_version}: unapproved source {source}")

    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"release-dependencies-ok packages={len(packages)} licenses={len(allowed)}")
    return 0


def version_tuple(version: str) -> tuple[int, ...]:
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(?:[-+].*)?", version)
    if not match:
        raise ValueError(f"unsupported semantic version {version!r}")
    return tuple(int(value) for value in match.groups())


def check_advisories() -> int:
    policy = load_json(POLICY_PATH)
    packages = resolved_packages(metadata())
    installed: dict[str, list[str]] = {}
    for package in packages:
        installed.setdefault(package["name"], []).append(package["version"])

    unresolved: list[str] = []
    for advisory in policy["resolved_advisories"]:
        versions = installed.get(advisory["package"], [])
        if not versions:
            continue
        patched = version_tuple(advisory["patched"])
        for version in versions:
            if version_tuple(version) < patched:
                unresolved.append(
                    f'{advisory["id"]}: {advisory["package"]} {version} '
                    f'< patched {advisory["patched"]}'
                )

    if unresolved:
        print("\n".join(unresolved), file=sys.stderr)
        return 1
    print(
        "release-advisories-ok open=0 "
        f'resolved={len(policy["resolved_advisories"])}'
    )
    return 0


def check_matrix() -> int:
    text = MATRIX_PATH.read_text(encoding="utf-8")
    required = (
        "Generic Wayland",
        "Niri",
        "KDE Plasma Wayland",
        "Peer.Ping",
        "Dogtail/pyatspi",
        "not verified on this host",
        "Phase 21D",
    )
    missing = [value for value in required if value not in text]
    if missing:
        print(f"release matrix missing: {', '.join(missing)}", file=sys.stderr)
        return 1
    rows = sum(1 for line in text.splitlines() if line.startswith("| ")) - 2
    if rows < 3:
        print("release matrix requires at least three environment rows", file=sys.stderr)
        return 1
    print(f"release-environment-matrix-ok environments={rows}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("check", choices=("dependencies", "advisories", "matrix"))
    args = parser.parse_args()
    return {
        "dependencies": check_dependencies,
        "advisories": check_advisories,
        "matrix": check_matrix,
    }[args.check]()


if __name__ == "__main__":
    raise SystemExit(main())
