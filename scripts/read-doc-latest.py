#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Print the latest docs version from a seeded site or docs/versions.json."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
SEMVER_RE = re.compile(
    r"^v(?P<major>\d+)\.(?P<minor>\d+)\.(?P<patch>\d+)"
    r"(?:-(?P<prerelease>[0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$"
)


def semver_key(version: str) -> tuple[int, int, int, tuple[tuple[int, int | str], ...]] | None:
    match = SEMVER_RE.fullmatch(version)
    if match is None:
        return None
    prerelease = match.group("prerelease")
    identifiers: tuple[tuple[int, int | str], ...] = ()
    if prerelease is not None:
        identifiers = tuple(
            (0, int(part)) if part.isdigit() else (1, part)
            for part in prerelease.split(".")
        )
    return (
        int(match.group("major")),
        int(match.group("minor")),
        int(match.group("patch")),
        identifiers,
    )


def select_latest(versions: list[str]) -> str:
    parsed = [(version, semver_key(version)) for version in versions]
    releases = [(version, key) for version, key in parsed if key is not None]
    stable = [(version, key) for version, key in releases if not key[3]]
    candidates = stable or releases
    if not candidates:
        return "main"
    return max(candidates, key=lambda item: item[1])[0]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--site", default="site")
    parser.add_argument(
        "--candidate",
        action="append",
        default=[],
        help="release version to include when resolving latest",
    )
    args = parser.parse_args()

    candidates = [Path(args.site) / "versions.json", ROOT / "docs" / "versions.json"]
    for path in candidates:
        if path.exists():
            data = json.loads(path.read_text(encoding="utf-8"))
            versions = [
                str(entry["id"])
                for entry in data.get("versions", [])
                if isinstance(entry, dict) and isinstance(entry.get("id"), str)
            ]
            versions.extend(args.candidate)
            print(select_latest(versions))
            return 0
    print(select_latest(args.candidate))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
