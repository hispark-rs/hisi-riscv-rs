#!/usr/bin/env python3
"""mdBook preprocessor for tutorial command snippets.

Tutorial pages use:

    {{#tutorial-snippet snippet_id}}

Snippets are extracted from scripts/tutorial-contracts.sh between
`# docs:start snippet_id` and `# docs:end`. This keeps tutorial command blocks
rendered from the executable contract script instead of copied by hand.
"""

from __future__ import annotations

import json
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
CONTRACTS = ROOT / "scripts" / "tutorial-contracts.sh"
TOKEN_RE = re.compile(r"\{\{#tutorial-snippet\s+([A-Za-z0-9_.-]+)\s*\}\}")


def load_snippets() -> dict[str, str]:
    snippets: dict[str, str] = {}
    current: str | None = None
    lines: list[str] = []

    for line in CONTRACTS.read_text(encoding="utf-8").splitlines():
        if line.startswith("# docs:start "):
            if current is not None:
                raise SystemExit(f"nested docs:start before docs:end for {current}")
            current = line.split(None, 2)[2].strip()
            lines = []
            continue
        if line == "# docs:end":
            if current is None:
                raise SystemExit("docs:end without docs:start")
            if current in snippets:
                raise SystemExit(f"duplicate tutorial snippet id: {current}")
            snippets[current] = "\n".join(lines).strip("\n")
            current = None
            lines = []
            continue
        if current is not None:
            lines.append(line)

    if current is not None:
        raise SystemExit(f"unterminated tutorial snippet: {current}")
    return snippets


def render(content: str, snippets: dict[str, str]) -> str:
    def replace(match: re.Match[str]) -> str:
        snippet_id = match.group(1)
        try:
            body = snippets[snippet_id]
        except KeyError as exc:
            raise SystemExit(f"unknown tutorial snippet id: {snippet_id}") from exc
        return f"```bash\n{body}\n```"

    return TOKEN_RE.sub(replace, content)


def walk(value: object, snippets: dict[str, str]) -> int:
    changed = 0
    if isinstance(value, dict):
        content = value.get("content")
        if isinstance(content, str):
            rewritten = render(content, snippets)
            if rewritten != content:
                value["content"] = rewritten
                changed += 1
        for child in value.values():
            changed += walk(child, snippets)
    elif isinstance(value, list):
        for child in value:
            changed += walk(child, snippets)
    return changed


def main() -> int:
    if len(sys.argv) >= 2 and sys.argv[1] == "supports":
        return 0

    payload = json.load(sys.stdin)
    if not isinstance(payload, list) or len(payload) != 2:
        raise SystemExit("expected mdBook preprocessor input [context, book]")

    _, book = payload

    snippets = load_snippets()
    walk(book, snippets)
    json.dump(book, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
