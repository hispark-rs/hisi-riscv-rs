#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Verify that GitHub Pages serves the docs artifact just deployed."""

from __future__ import annotations

import argparse
import json
import time
from urllib.error import HTTPError, URLError
from urllib.parse import urljoin
from urllib.request import Request, urlopen


DEFAULT_BASE = "https://hispark-rs.github.io/hisi-riscv-rs/"


def fetch(url: str, attempt: int) -> bytes:
    separator = "&" if "?" in url else "?"
    request = Request(
        f"{url}{separator}deployment_check={attempt}",
        headers={"Cache-Control": "no-cache", "User-Agent": "hisi-docs-deploy-check"},
    )
    with urlopen(request, timeout=20) as response:
        if response.status != 200:
            raise RuntimeError(f"{url}: HTTP {response.status}")
        return response.read()


def verify(base: str, version: str, latest_version: str | None, attempt: int) -> None:
    manifest = json.loads(fetch(urljoin(base, "versions.json"), attempt))
    entries = {
        entry.get("id"): entry
        for entry in manifest.get("versions", [])
        if isinstance(entry, dict)
    }
    if version not in entries:
        raise RuntimeError(f"versions.json does not list {version}")
    if latest_version is not None:
        if manifest.get("latest") != latest_version:
            raise RuntimeError(
                f"latest is {manifest.get('latest')!r}, expected {latest_version!r}"
            )
        latest = entries.get("latest")
        if latest is None or latest.get("target") != latest_version:
            raise RuntimeError("latest alias target does not match the release")

    for path in (
        f"{version}/index.html",
        f"api/{version}/ws63/index.html",
        f"api/{version}/ws63/hisi_hal/index.html",
    ):
        fetch(urljoin(base, path), attempt)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", default=DEFAULT_BASE)
    parser.add_argument("--version", required=True)
    parser.add_argument("--latest-version")
    parser.add_argument("--attempts", type=int, default=12)
    parser.add_argument("--delay", type=float, default=5.0)
    args = parser.parse_args()

    base = args.base_url.rstrip("/") + "/"
    last_error: Exception | None = None
    for attempt in range(1, args.attempts + 1):
        try:
            verify(base, args.version, args.latest_version, attempt)
        except (HTTPError, URLError, RuntimeError, json.JSONDecodeError) as error:
            last_error = error
            print(f"live docs attempt {attempt}/{args.attempts}: {error}")
            if attempt < args.attempts:
                time.sleep(args.delay)
        else:
            print(f"live docs verified: {base} version={args.version}")
            return 0

    raise SystemExit(f"live docs verification failed: {last_error}")


if __name__ == "__main__":
    raise SystemExit(main())
