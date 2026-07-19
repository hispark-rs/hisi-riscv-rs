#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Read one pinned WPA archive hash from the WS63 task profile."""

from __future__ import annotations

import argparse
from pathlib import Path
import tomllib


ARTIFACT_BY_PROFILE = {
    "wpa2-personal": "wpa2-personal-oracle",
    "wpa3-personal": "wpa3-personal-candidate",
}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("task_profile", type=Path)
    parser.add_argument("wpa_profile", choices=sorted(ARTIFACT_BY_PROFILE))
    args = parser.parse_args()

    profile = tomllib.loads(args.task_profile.read_text(encoding="utf-8"))
    artifact_id = ARTIFACT_BY_PROFILE[args.wpa_profile]
    matches = [
        artifact["sha256"]
        for artifact in profile["artifacts"]
        if artifact["id"] == artifact_id
    ]
    if len(matches) != 1:
        raise SystemExit(f"task profile must define exactly one {artifact_id}")
    print(matches[0])


if __name__ == "__main__":
    main()
