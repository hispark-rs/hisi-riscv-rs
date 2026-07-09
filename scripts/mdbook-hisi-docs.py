#!/usr/bin/env python3
"""mdBook preprocessor for hisi-riscv-rs docs.

This is the single mdBook preprocessor for user-facing docs:
- renders tutorial snippets from scripts/tutorial-contracts.sh;
- renders chip-aware snippet variants;
- injects chip/version metadata for the selector UI;
- provides small chip-field and API-link helpers.
"""

from __future__ import annotations

import html
import json
import os
from pathlib import Path
import re
import sys
import tomllib
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
CONTRACTS = ROOT / "scripts" / "tutorial-contracts.sh"
CHIPS_TOML = ROOT / "docs" / "chips.toml"
VERSIONS_JSON = ROOT / "docs" / "versions.json"

SNIPPET_RE = re.compile(r"\{\{#tutorial-snippet\s+([A-Za-z0-9_.-]+)\s*\}\}")
CHIP_FIELD_RE = re.compile(r"\{\{#chip-field\s+([A-Za-z0-9_.-]+)\s*\}\}")
CHIP_IF_RE = re.compile(
    r"\{\{#chip-if\s+([A-Za-z0-9_.-]+)\s*\}\}(.*?)"
    r"(?:\{\{#chip-else\}\}(.*?))?\{\{#chip-end\}\}",
    re.DOTALL,
)
API_LINK_RE = re.compile(r"\{\{#api-link\s+([^|}]+)\|([^}]+)\}\}")


def load_chips() -> dict[str, Any]:
    data = tomllib.loads(CHIPS_TOML.read_text(encoding="utf-8"))
    chips = data.get("chips")
    if not isinstance(chips, dict):
        raise SystemExit("docs/chips.toml is missing [chips.*] entries")
    default = data.get("default")
    selectable = data.get("selectable", [])
    if default not in chips:
        raise SystemExit(f"default chip is not defined: {default}")
    return {
        "default": default,
        "selectable": selectable,
        "chips": chips,
    }


def load_versions() -> dict[str, Any]:
    data = json.loads(VERSIONS_JSON.read_text(encoding="utf-8"))
    current = os.environ.get("DOC_VERSION")
    latest = os.environ.get("LATEST_VERSION")
    if current:
        ids = {entry.get("id") for entry in data.get("versions", [])}
        if current not in ids:
            kind = "release" if current.startswith("v") else "branch"
            data.setdefault("versions", []).append(
                {
                    "id": current,
                    "label": current,
                    "kind": kind,
                    "book_base": f"/hisi-riscv-rs/{current}/",
                    "api_base": f"/hisi-riscv-rs/api/{current}/",
                }
            )
    if latest:
        data["latest"] = latest
        for entry in data.get("versions", []):
            if entry.get("id") == "latest":
                entry["target"] = latest
    return data


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


def html_code_block(body: str) -> str:
    return f'<pre><code class="language-bash">{html.escape(body)}</code></pre>'


def manifest_script(chips: dict[str, Any], versions: dict[str, Any]) -> str:
    payload = json.dumps({"chips": chips, "versions": versions}, ensure_ascii=False)
    payload = payload.replace("</", "<\\/")
    return f'<script type="application/json" data-hisi-docs-manifest>{payload}</script>\n\n'


def render_snippets(content: str, snippets: dict[str, str], chips: dict[str, Any]) -> str:
    selectable = chips["selectable"]

    def replace(match: re.Match[str]) -> str:
        snippet_id = match.group(1)
        variants = {
            chip: snippets[f"{snippet_id}.{chip}"]
            for chip in selectable
            if f"{snippet_id}.{chip}" in snippets
        }
        if variants:
            blocks = []
            for chip, body in variants.items():
                blocks.append(
                    f'<div class="hisi-chip-block" data-hisi-chip="{html.escape(chip)}">'
                    f"{html_code_block(body)}</div>"
                )
            return "\n".join(blocks)

        try:
            body = snippets[snippet_id]
        except KeyError as exc:
            raise SystemExit(f"unknown tutorial snippet id: {snippet_id}") from exc
        return f"```bash\n{body}\n```"

    return SNIPPET_RE.sub(replace, content)


def render_chip_fields(content: str, chips: dict[str, Any]) -> str:
    default_chip = chips["default"]
    default_data = chips["chips"][default_chip]

    def replace(match: re.Match[str]) -> str:
        field = match.group(1)
        value = default_data.get(field)
        if value is None:
            raise SystemExit(f"unknown chip field: {field}")
        return (
            f'<span data-hisi-chip-field="{html.escape(field)}">'
            f"{html.escape(str(value))}</span>"
        )

    return CHIP_FIELD_RE.sub(replace, content)


def render_chip_ifs(content: str, chips: dict[str, Any]) -> str:
    default_chip = chips["default"]

    def replace(match: re.Match[str]) -> str:
        chip = match.group(1)
        if chip not in chips["chips"]:
            raise SystemExit(f"unknown chip in chip-if: {chip}")
        then_body = match.group(2).strip()
        else_body = (match.group(3) or "").strip()
        blocks = [
            f'<div class="hisi-chip-block" data-hisi-chip="{html.escape(chip)}">{then_body}</div>'
        ]
        if else_body:
            others = [c for c in chips["selectable"] if c != chip]
            for other in others:
                blocks.append(
                    f'<div class="hisi-chip-block" data-hisi-chip="{html.escape(other)}">'
                    f"{else_body}</div>"
                )
        elif chip != default_chip:
            blocks[0] = blocks[0].replace('class="hisi-chip-block"', 'class="hisi-chip-block hidden"')
        return "\n".join(blocks)

    return CHIP_IF_RE.sub(replace, content)


def render_api_links(content: str, chips: dict[str, Any], versions: dict[str, Any]) -> str:
    default_chip = chips["default"]
    api_base = next(
        (v["api_base"] for v in versions.get("versions", []) if v.get("id") == versions.get("default")),
        "/hisi-riscv-rs/api/latest/",
    )

    def replace(match: re.Match[str]) -> str:
        path = match.group(1).strip()
        label = match.group(2).strip()
        href = f"{api_base.rstrip('/')}/{default_chip}/{path.lstrip('/')}"
        return (
            f'<a data-hisi-api-link="{html.escape(path)}" '
            f'href="{html.escape(href)}">{html.escape(label)}</a>'
        )

    return API_LINK_RE.sub(replace, content)


def render(content: str, snippets: dict[str, str], chips: dict[str, Any], versions: dict[str, Any]) -> str:
    content = render_chip_ifs(content, chips)
    content = render_snippets(content, snippets, chips)
    content = render_chip_fields(content, chips)
    content = render_api_links(content, chips, versions)
    return manifest_script(chips, versions) + content


def walk(value: object, snippets: dict[str, str], chips: dict[str, Any], versions: dict[str, Any]) -> int:
    changed = 0
    if isinstance(value, dict):
        content = value.get("content")
        if isinstance(content, str):
            value["content"] = render(content, snippets, chips, versions)
            changed += 1
        for child in value.values():
            changed += walk(child, snippets, chips, versions)
    elif isinstance(value, list):
        for child in value:
            changed += walk(child, snippets, chips, versions)
    return changed


def main() -> int:
    if len(sys.argv) >= 2 and sys.argv[1] == "supports":
        return 0

    payload = json.load(sys.stdin)
    if not isinstance(payload, list) or len(payload) != 2:
        raise SystemExit("expected mdBook preprocessor input [context, book]")

    _, book = payload
    snippets = load_snippets()
    chips = load_chips()
    versions = load_versions()
    walk(book, snippets, chips, versions)
    json.dump(book, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
