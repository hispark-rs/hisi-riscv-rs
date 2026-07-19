#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Generate docs/versions.json from local git tags."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VERSION_RE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$")


def semver_key(version: str) -> tuple[int, int, int, int, str]:
    match = VERSION_RE.match(version)
    if not match:
        raise ValueError(version)
    major, minor, patch = (int(match.group(i)) for i in range(1, 4))
    pre = match.group(4)
    stable_rank = 1 if pre is None else 0
    return (major, minor, patch, stable_rank, pre or "")


def git_tags() -> list[str]:
    result = subprocess.run(
        ["git", "tag", "--list", "v*"],
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    return [line.strip() for line in result.stdout.splitlines() if VERSION_RE.match(line.strip())]


def release_entry(version: str, project_base: str) -> dict[str, str]:
    return {
        "id": version,
        "label": version,
        "kind": "release",
        "book_base": f"{project_base}/{version}/",
        "api_base": f"{project_base}/api/{version}/",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", default="docs/versions.json")
    parser.add_argument("--project-base", default="/hisi-riscv-rs")
    parser.add_argument("--include-main", action="store_true", default=True)
    parser.add_argument("--no-main", dest="include_main", action="store_false")
    args = parser.parse_args()

    project_base = args.project_base.rstrip("/")
    tags = sorted(git_tags(), key=semver_key, reverse=True)
    stable_tags = [tag for tag in tags if "-" not in tag]
    latest = stable_tags[0] if stable_tags else (tags[0] if tags else "main")

    versions: list[dict[str, str]] = [
        {
            "id": "latest",
            "label": "Latest",
            "kind": "alias",
            "target": latest,
            "book_base": f"{project_base}/latest/",
            "api_base": f"{project_base}/api/latest/",
        }
    ]
    if args.include_main:
        versions.append(
            {
                "id": "main",
                "label": "main",
                "kind": "branch",
                "book_base": f"{project_base}/main/",
                "api_base": f"{project_base}/api/main/",
            }
        )
    versions.extend(release_entry(tag, project_base) for tag in tags)

    output = ROOT / args.output
    output.write_text(
        json.dumps({"default": "latest", "latest": latest, "versions": versions}, ensure_ascii=False, indent=2)
        + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
