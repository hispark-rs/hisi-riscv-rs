#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Assemble mdBook output into the versioned GitHub Pages layout."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import shutil


ROOT = Path(__file__).resolve().parents[1]
VERSIONS_JSON = ROOT / "docs" / "versions.json"


def copytree(src: Path, dst: Path) -> None:
    if dst.exists():
        shutil.rmtree(dst)
    shutil.copytree(src, dst)


def redirect(target: str) -> str:
    return f'<!doctype html><meta http-equiv="refresh" content="0; url={target}">\n'


def version_exists(out: Path, version_id: str, current_version: str) -> bool:
    return version_id == current_version or (out / version_id / "index.html").exists()


def load_manifest(out: Path) -> dict[str, object]:
    source = json.loads(VERSIONS_JSON.read_text(encoding="utf-8"))
    existing_path = out / "versions.json"
    if not existing_path.exists():
        return source

    existing = json.loads(existing_path.read_text(encoding="utf-8"))
    by_id = {
        entry["id"]: entry
        for entry in existing.get("versions", [])
        if isinstance(entry, dict) and isinstance(entry.get("id"), str)
    }
    for entry in source.get("versions", []):
        if isinstance(entry, dict) and isinstance(entry.get("id"), str):
            by_id[entry["id"]] = entry
    existing["versions"] = list(by_id.values())
    existing.setdefault("default", source.get("default", "latest"))
    existing.setdefault("latest", source.get("latest", "main"))
    return existing


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", default="main")
    parser.add_argument("--book", default="docs/book")
    parser.add_argument("--out", default="site")
    parser.add_argument("--latest-version", default=None)
    args = parser.parse_args()

    version = args.version
    book = Path(args.book)
    out = Path(args.out)
    latest_version = args.latest_version or version
    if not book.exists():
        raise SystemExit(f"book output does not exist: {book}")

    out.mkdir(parents=True, exist_ok=True)
    copytree(book, out / version)

    data = load_manifest(out)
    ids = {entry["id"] for entry in data.get("versions", [])}
    if version not in ids:
        kind = "release" if re.match(r"^v\d+\.\d+\.\d+", version) else "branch"
        data.setdefault("versions", []).append(
            {
                "id": version,
                "label": version,
                "kind": kind,
                "book_base": f"/hisi-riscv-rs/{version}/",
                "api_base": f"/hisi-riscv-rs/api/{version}/",
            }
        )
    data["latest"] = latest_version
    for entry in data.get("versions", []):
        if entry.get("id") == "latest":
            entry["target"] = latest_version

    data["versions"] = [
        entry
        for entry in data.get("versions", [])
        if entry.get("id") == "latest" or version_exists(out, str(entry.get("id")), version)
    ]
    ids = {entry.get("id") for entry in data.get("versions", [])}
    if latest_version not in ids:
        latest_version = version
        data["latest"] = latest_version
        for entry in data.get("versions", []):
            if entry.get("id") == "latest":
                entry["target"] = latest_version

    (out / "versions.json").write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (out / "index.html").write_text(redirect("latest/"), encoding="utf-8")

    if version == latest_version:
        copytree(book, out / "latest")
    elif not (out / "latest").exists():
        (out / "latest").mkdir(parents=True)
        (out / "latest" / "index.html").write_text(redirect(f"../{latest_version}/"), encoding="utf-8")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
