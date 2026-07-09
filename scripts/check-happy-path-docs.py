#!/usr/bin/env python3
"""Reject happy-path documentation drift.

The user-facing happy path must stay aligned with scripts/happy-path-smoke.sh and
the hisi-rs-template CI. This check catches stale install, dependency, and direct
download recipes before they become tutorial truth.
"""

from __future__ import annotations

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
CHECK_ROOTS = [
    ROOT / "README.md",
    ROOT / "docs" / "src",
    ROOT / "crates" / "hisi-rs-template",
]

PATTERNS = [
    (
        re.compile(r"cargo\s+(?:\+stable\s+)?install\s+--git\s+https://github\.com/hispark-rs/hisi-fwpkg\b"),
        "install hisi-fwpkg-cli from crates.io, not the git repo, in the happy path",
    ),
    (
        re.compile(r"cargo\s+(?:\+stable\s+)?install\s+hisi-fwpkg-cli(?!\s+--version\s+0\.3\.0)"),
        "pin hisi-fwpkg-cli 0.3.0 in tested happy-path docs",
    ),
    (
        re.compile(r"probe-rs\s+download(?!.*--binary-format\s+bin).*target/riscv32imfc", re.IGNORECASE),
        "probe-rs smoke/download docs must use plan image + --binary-format bin",
    ),
    (
        re.compile(r"hisi-riscv-hal\s*=\s*[{ ]version\s*=\s*\"0\.(?:4|5|6)(?:\"|[^\"]*\"(?!0-alpha\.1))"),
        "template happy path must not drift to an old HAL dependency",
    ),
    (
        re.compile(r"hisi-riscv-rt\s*=\s*[{ ]version\s*=\s*\"0\.[0-4]"),
        "template happy path must not drift to an old runtime dependency",
    ),
]

ALLOW = [
    re.compile(r"Historical Notes"),
    re.compile(r"docs/review/"),
    re.compile(r"0\.5\.0\+ has NO default chip"),
    re.compile(r"hisi-fwpkg-cli 0\.3\.0"),
]


def iter_files() -> list[Path]:
    files: list[Path] = []
    for root in CHECK_ROOTS:
        if root.is_file():
            files.append(root)
            continue
        if not root.exists():
            continue
        for path in root.rglob("*"):
            if path.is_file() and (path.suffix in {".md", ".toml", ".yml", ".yaml", ".sh"} or path.name == "justfile"):
                files.append(path)
    return sorted(set(files))


def main() -> int:
    errors: list[str] = []
    for path in iter_files():
        rel = path.relative_to(ROOT)
        text = path.read_text(encoding="utf-8", errors="ignore")
        for lineno, line in enumerate(text.splitlines(), start=1):
            rendered = f"{rel}:{lineno}:{line}"
            if any(pattern.search(rendered) for pattern in ALLOW):
                continue
            for pattern, reason in PATTERNS:
                if pattern.search(line):
                    errors.append(f"{rel}:{lineno}: {reason}: {line.strip()}")

    if errors:
        print("happy-path documentation drift detected:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

