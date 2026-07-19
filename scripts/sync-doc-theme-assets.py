#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Refresh shared mdBook theme assets across retained version directories.

Versioned docs keep old handbook snapshots under /vX.Y.Z/ and /latest/. The
chip/version selector is a shared UI shell, so fixes to that JS/CSS should apply
to retained snapshots without rebuilding their Markdown content.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import shutil


ASSETS = ("js", "css")


def single_asset(theme: Path, suffix: str) -> Path:
    matches = sorted(theme.glob(f"hisi-docs-*.{suffix}"))
    if len(matches) != 1:
        raise SystemExit(f"expected one hisi-docs-*.{suffix} in {theme}, found {len(matches)}")
    return matches[0]


def rewrite_html(version_dir: Path, suffix: str, asset_name: str) -> int:
    pattern = re.compile(rf"theme/hisi-docs-[A-Za-z0-9]+\.{re.escape(suffix)}")
    changed = 0
    for html_path in version_dir.rglob("*.html"):
        text = html_path.read_text(encoding="utf-8", errors="ignore")
        new_text = pattern.sub(f"theme/{asset_name}", text)
        if new_text != text:
            html_path.write_text(new_text, encoding="utf-8")
            changed += 1
    return changed


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--book", default="docs/book")
    parser.add_argument("--site", default="site")
    args = parser.parse_args()

    book = Path(args.book)
    site = Path(args.site)
    source_theme = book / "theme"
    if not source_theme.exists():
        raise SystemExit(f"book theme directory does not exist: {source_theme}")
    if not site.exists():
        raise SystemExit(f"site directory does not exist: {site}")

    source_assets = {suffix: single_asset(source_theme, suffix) for suffix in ASSETS}
    refreshed = 0

    for version_dir in sorted(path for path in site.iterdir() if path.is_dir()):
        if version_dir.name == "api":
            continue
        theme = version_dir / "theme"
        if not theme.exists():
            continue

        for suffix, source in source_assets.items():
            for old in theme.glob(f"hisi-docs-*.{suffix}"):
                old.unlink()
            shutil.copy2(source, theme / source.name)
            rewrite_html(version_dir, suffix, source.name)
        refreshed += 1

    print(f"sync-doc-theme-assets: refreshed {refreshed} version directories")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
