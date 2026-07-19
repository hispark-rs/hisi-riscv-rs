#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Validate docs version manifest and optional built site layout."""

from __future__ import annotations

from pathlib import Path
import json
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
VERSIONS_JSON = ROOT / "docs" / "versions.json"
SEMVER_RE = re.compile(r"^v\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$")
PRE_BLOCK_RE = re.compile(r"<pre\b.*?</pre>", re.DOTALL | re.IGNORECASE)


def strip_code_blocks(html: str) -> str:
    return PRE_BLOCK_RE.sub("", html)


def main() -> int:
    site = Path(sys.argv[1]) if len(sys.argv) > 1 else None
    manifest = site / "versions.json" if site is not None else VERSIONS_JSON
    if not manifest.exists():
        manifest = VERSIONS_JSON
    data = json.loads(manifest.read_text(encoding="utf-8"))
    errors: list[str] = []

    versions = data.get("versions")
    default = data.get("default")
    latest = data.get("latest")
    if not isinstance(versions, list) or not versions:
        errors.append("docs/versions.json must contain a non-empty versions list")
        versions = []
    ids = [v.get("id") for v in versions if isinstance(v, dict)]
    if default not in ids:
        errors.append(f"default version is not listed: {default}")
    if latest not in ids:
        errors.append(f"latest target is not listed: {latest}")

    for version in versions:
        if not isinstance(version, dict):
            errors.append("version entries must be objects")
            continue
        version_id = version.get("id")
        kind = version.get("kind")
        if kind not in {"alias", "branch", "release"}:
            errors.append(f"{version_id}: invalid kind {kind!r}")
        if kind == "release" and not SEMVER_RE.match(str(version_id)):
            errors.append(f"{version_id}: release ids must look like vX.Y.Z")
        for field in ("book_base", "api_base"):
            value = version.get(field)
            if not isinstance(value, str) or not value.startswith("/"):
                errors.append(f"{version_id}: {field} must be an absolute site path")
        if kind == "alias" and version.get("target") not in ids:
            errors.append(f"{version_id}: alias target is not listed")

    if site is not None:
        if not (site / "index.html").exists():
            errors.append(f"{site}: missing root index.html")
        if not (site / "versions.json").exists():
            errors.append(f"{site}: missing versions.json")
        for version_id in ids:
            if version_id in {"latest"}:
                continue
            if not (site / str(version_id) / "index.html").exists():
                errors.append(f"{site}: missing {version_id}/index.html")
        if (site / "main").exists() and not (site / "main" / "index.html").exists():
            errors.append(f"{site}: main/ exists but has no index.html")
        if (site / "api").exists() and not any((site / "api").iterdir()):
            errors.append(f"{site}: api/ exists but is empty")
        if (site / "api" / "main").exists():
            for chip in ("ws63", "bs21"):
                if not (site / "api" / "main" / chip / "index.html").exists():
                    errors.append(f"{site}: missing api/main/{chip}/index.html")
        for version_id in ids:
            if version_id in {"latest"}:
                continue
            api_version = site / "api" / str(version_id)
            if api_version.exists():
                for chip in ("ws63", "bs21"):
                    if not (api_version / chip / "index.html").exists():
                        errors.append(f"{site}: missing api/{version_id}/{chip}/index.html")
        for html_path in site.rglob("*.html"):
            text = html_path.read_text(encoding="utf-8", errors="ignore")
            if "{{#tutorial-snippet" in text or "{{#chip-" in text:
                errors.append(f"{html_path}: unexpanded docs token")
            if re.search(r"(?m)^\s*#{1,6}\s+\S", strip_code_blocks(text)):
                errors.append(f"{html_path}: unrendered Markdown heading")

    if errors:
        print("versioned docs errors:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
