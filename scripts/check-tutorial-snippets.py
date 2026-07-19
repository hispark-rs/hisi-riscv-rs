#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Validate tutorial snippets and chip-aware docs tokens."""

from __future__ import annotations

from pathlib import Path
import re
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[1]
CONTRACTS = ROOT / "scripts" / "tutorial-contracts.sh"
CHIPS_TOML = ROOT / "docs" / "chips.toml"
TUTORIALS = ROOT / "docs" / "src" / "tutorials"
TOKEN_RE = re.compile(r"\{\{#tutorial-snippet\s+([A-Za-z0-9_.-]+)\s*\}\}")
CHIP_FIELD_RE = re.compile(r"\{\{#chip-field\s+([A-Za-z0-9_.-]+)\s*\}\}")
CHIP_IF_RE = re.compile(r"\{\{#chip-if\s+([A-Za-z0-9_.-]+)\s*\}\}")
API_LINK_RE = re.compile(r"\{\{#api-link\s+([^|}]+)\|([^}]+)\}\}")


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


def load_chips() -> dict[str, object]:
    return tomllib.loads(CHIPS_TOML.read_text(encoding="utf-8"))


def main() -> int:
    snippets, errors = parse_snippets()
    chips_doc = load_chips()
    chips = chips_doc.get("chips", {})
    selectable = set(chips_doc.get("selectable", []))
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
                has_variants = any(key.startswith(f"{snippet_id}.") for key in snippets)
                if snippet_id not in snippets and not has_variants:
                    errors.append(f"{rel}:{lineno}: unknown tutorial snippet id: {snippet_id}")
            for match in CHIP_FIELD_RE.finditer(line):
                field = match.group(1)
                for chip, data in chips.items():
                    if chip in selectable and field not in data:
                        errors.append(f"{rel}:{lineno}: chip field {field!r} missing for {chip}")
            for match in CHIP_IF_RE.finditer(line):
                chip = match.group(1)
                if chip not in chips:
                    errors.append(f"{rel}:{lineno}: unknown chip in chip-if: {chip}")
            for match in API_LINK_RE.finditer(line):
                path_part = match.group(1).strip()
                if not path_part:
                    errors.append(f"{rel}:{lineno}: empty api-link path")

    unused = []
    for snippet_id in snippets:
        base = snippet_id.rsplit(".", 1)[0] if snippet_id.rsplit(".", 1)[-1] in selectable else snippet_id
        if base not in used and snippet_id not in used:
            unused.append(snippet_id)
    unused = sorted(unused)
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
