#!/usr/bin/env python3
"""Validate tutorial snippets and prevent copied tutorial command blocks."""

from __future__ import annotations

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
CONTRACTS = ROOT / "scripts" / "tutorial-contracts.sh"
TUTORIALS = ROOT / "docs" / "src" / "tutorials"
TOKEN_RE = re.compile(r"\{\{#tutorial-snippet\s+([A-Za-z0-9_.-]+)\s*\}\}")


def parse_snippets() -> tuple[dict[str, list[str]], list[str]]:
    snippets: dict[str, list[str]] = {}
    errors: list[str] = []
    current: str | None = None
    lines: list[str] = []

    for lineno, line in enumerate(CONTRACTS.read_text(encoding="utf-8").splitlines(), start=1):
        if line.startswith("# docs:start "):
            if current is not None:
                errors.append(f"{CONTRACTS}:{lineno}: nested docs:start before docs:end for {current}")
                continue
            current = line.split(None, 2)[2].strip()
            lines = []
            continue
        if line == "# docs:end":
            if current is None:
                errors.append(f"{CONTRACTS}:{lineno}: docs:end without docs:start")
                continue
            if current in snippets:
                errors.append(f"{CONTRACTS}:{lineno}: duplicate tutorial snippet id: {current}")
            elif not any(part.strip() for part in lines):
                errors.append(f"{CONTRACTS}:{lineno}: empty tutorial snippet: {current}")
            else:
                snippets[current] = lines[:]
            current = None
            lines = []
            continue
        if current is not None:
            lines.append(line)

    if current is not None:
        errors.append(f"{CONTRACTS}: unterminated tutorial snippet: {current}")
    return snippets, errors


def iter_tutorials() -> list[Path]:
    return sorted(path for path in TUTORIALS.rglob("*.md") if path.is_file())


def main() -> int:
    snippets, errors = parse_snippets()
    used: set[str] = set()

    for path in iter_tutorials():
        rel = path.relative_to(ROOT)
        text = path.read_text(encoding="utf-8")
        for lineno, line in enumerate(text.splitlines(), start=1):
            if line.strip() in {"```bash", "```sh", "```shell"}:
                errors.append(
                    f"{rel}:{lineno}: tutorials must use {{#tutorial-snippet ...}} "
                    "instead of handwritten shell code blocks"
                )
            for match in TOKEN_RE.finditer(line):
                snippet_id = match.group(1)
                used.add(snippet_id)
                if snippet_id not in snippets:
                    errors.append(f"{rel}:{lineno}: unknown tutorial snippet id: {snippet_id}")

    unused = sorted(set(snippets) - used)
    if unused:
        errors.append("unused tutorial snippet ids: " + ", ".join(unused))

    if errors:
        print("tutorial snippet drift detected:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
