#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Reject stale HiSilicon image-format workflow claims.

The executable image semantics live in hisi-fwpkg. Repository docs and scripts may
describe transfer/reset channels, but should not reintroduce the old "probe-rs
direct ELF download" smoke path as if it were the recommended download flow.
"""

from __future__ import annotations

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]

CHECK_ROOTS = [
    ROOT / "docs" / "src",
    ROOT / "hil",
    ROOT / "crates" / "hisi-rs-template",
]

STALE_PATTERNS = [
    (re.compile(r"probe-rs\s+download\s+<elf>", re.IGNORECASE), "probe-rs direct ELF placeholder"),
    (re.compile(r"直接烧裸\s*ELF"), "direct bare-ELF flashing claim"),
    (re.compile(r"裸\s*ELF\s*直接\s*download"), "direct bare-ELF download claim"),
    (re.compile(r"无中间\s*\.img"), "old no-image WS63 workflow claim"),
    (re.compile(r"没有中间\s*\.img"), "old no-image WS63 workflow claim"),
    (re.compile(r"--binary-format\s+bin.*仅.*BS2X"), "binary-format incorrectly scoped to BS2X"),
    (re.compile(r"--base-address.*仅.*BS2X"), "base-address incorrectly scoped to BS2X"),
    (re.compile(r"\bOther\(v\)"), "stale fwpkg partition fallback name"),
]

ALLOW_LINE_PATTERNS = [
    re.compile(r"不再直接烧"),
    re.compile(r"不要.*直接烧"),
]


def iter_files() -> list[Path]:
    files: list[Path] = []
    for root in CHECK_ROOTS:
        if not root.exists():
            continue
        for path in root.rglob("*"):
            if path.is_file() and path.suffix in {".md", ".sh", ".toml", ".justfile"}:
                files.append(path)
            elif path.is_file() and path.name == "justfile":
                files.append(path)
    return sorted(set(files))


def main() -> int:
    errors: list[str] = []
    for path in iter_files():
        rel = path.relative_to(ROOT)
        text = path.read_text(encoding="utf-8", errors="ignore")
        for lineno, line in enumerate(text.splitlines(), start=1):
            if any(pattern.search(line) for pattern in ALLOW_LINE_PATTERNS):
                continue
            for pattern, reason in STALE_PATTERNS:
                if pattern.search(line):
                    errors.append(f"{rel}:{lineno}: {reason}: {line.strip()}")

    if errors:
        print("image-format truth-source drift detected:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
