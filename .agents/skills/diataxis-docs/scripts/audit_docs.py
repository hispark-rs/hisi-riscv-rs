#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Mechanical documentation drift scanner for mdBook-style repos.

This script intentionally reports leads, not final judgments.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from urllib.parse import urldefrag


DRIFT_PATTERNS = [
    ("old_agent_skill_path", re.compile(r"\.claude/skills")),
    ("old_hil_count", re.compile(r"\b(?:15/15|19/19)\b")),
    ("old_example_count", re.compile(r"\b14\s*(?:个|例)\b")),
    ("old_default_members", re.compile(r"(?:libraries \+ blinky|库 \+ blinky|default-members\s*=\s*.*blinky)")),
    ("old_hil_marker_title", re.compile(r"HIL 标记串与环境变量")),
]

CURRENT_CLAIM_RE = re.compile(r"(?:当前|截至|latest|current|only|唯一|默认|stable|STABLE|UNSTABLE)")

MODE_HINTS = {
    "reference": [
        re.compile(r"为什么|原理|哲学|背景|取舍|评审发现|改进项"),
    ],
    "explanation": [
        re.compile(r"^## 环境变量|^## 命令|^## 字段|成功标记串|失败标记串"),
    ],
    "how-to": [
        re.compile(r"完整清单|API 清单|字段布局|架构评审"),
    ],
    "tutorials": [
        re.compile(r"完整清单|矩阵|API 清单|字段布局"),
    ],
}

LINK_RE = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")


def iter_markdown(root: Path):
    for path in sorted(root.rglob("*.md")):
        if any(part in {".git", "book", "target"} for part in path.parts):
            continue
        yield path


def category(path: Path) -> str | None:
    parts = path.parts
    for name in ("tutorials", "how-to", "reference", "explanation"):
        if name in parts:
            return name
    return None


def report(kind: str, path: Path, line_no: int, message: str):
    print(f"{path}:{line_no}: {kind}: {message}")


def scan_patterns(path: Path, text: str, include_current_claims: bool):
    cat = category(path)
    for i, line in enumerate(text.splitlines(), 1):
        for name, pattern in DRIFT_PATTERNS:
            if pattern.search(line):
                report(name, path, i, line.strip()[:180])
        if include_current_claims and CURRENT_CLAIM_RE.search(line):
            report("maybe_current_claim", path, i, line.strip()[:180])
        if cat in MODE_HINTS:
            for pattern in MODE_HINTS[cat]:
                if pattern.search(line):
                    report("diataxis_fit_lead", path, i, f"{cat} page contains: {line.strip()[:160]}")


def scan_links(path: Path, text: str):
    for i, line in enumerate(text.splitlines(), 1):
        for raw in LINK_RE.findall(line):
            target = raw.split()[0].strip("<>")
            if target.startswith(("http://", "https://", "mailto:", "#")):
                continue
            target, _fragment = urldefrag(target)
            if not target or target.startswith("#"):
                continue
            if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target):
                continue
            resolved = (path.parent / target).resolve()
            if not resolved.exists():
                report("missing_link_target", path, i, raw)


def main() -> int:
    parser = argparse.ArgumentParser(description="Scan docs for Diataxis/drift leads.")
    parser.add_argument("root", nargs="?", default="docs", help="docs root or repository root")
    parser.add_argument("--links", action="store_true", help="also check local Markdown link targets")
    parser.add_argument("--current-claims", action="store_true", help="also report lines with current/default/stable-style claims")
    args = parser.parse_args()

    root = Path(args.root)
    if not root.exists():
        print(f"error: root does not exist: {root}", file=sys.stderr)
        return 2

    docs_root = root / "src" if (root / "src").is_dir() else root
    count = 0
    for path in iter_markdown(docs_root):
        text = path.read_text(encoding="utf-8")
        scan_patterns(path, text, args.current_claims)
        if args.links:
            scan_links(path, text)
        count += 1

    print(f"scanned {count} markdown files under {docs_root}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
