#!/usr/bin/env python3
"""Print the latest docs version from a seeded site or docs/versions.json."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--site", default="site")
    args = parser.parse_args()

    candidates = [Path(args.site) / "versions.json", ROOT / "docs" / "versions.json"]
    for path in candidates:
        if path.exists():
            data = json.loads(path.read_text(encoding="utf-8"))
            print(data.get("latest") or data.get("default") or "main")
            return 0
    print("main")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
