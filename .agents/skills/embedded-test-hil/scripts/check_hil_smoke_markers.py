#!/usr/bin/env python3
"""Check WS63 HIL smoke grep patterns across script, docs, and skill wrappers."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]
SMOKE = ROOT / "hil/hil-smoke.sh"
REFERENCE = ROOT / "docs/src/reference/07-hil-markers.md"
SKILL_WRAPPER = ROOT / ".agents/skills/hil-smoke/hil.sh"
PARITY = ROOT / ".agents/skills/qemu-vs-hil/parity.sh"


CHECK_RE = re.compile(r'check\s+([A-Za-z0-9_]+)\s+"([^"]+)"')
REF_ROW_RE = re.compile(r"^\|\s*`([^`]+)`\s*\|\s*`([^`]+)`\s*\|", re.M)
CASE_RE = re.compile(r"([A-Za-z0-9_]+)\)\s+echo\s+\"([^\"]+)\"\s+;;")


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        print(f"missing file: {path}", file=sys.stderr)
        raise


def ws63_skill_pattern(raw: str) -> str:
    return raw.replace("$BANNER", "WS63")


def marker_for_body(text: str, path: Path) -> str:
    start = text.find("marker_for()")
    if start < 0:
        raise ValueError(f"{path}: marker_for() not found")
    open_brace = text.find("{", start)
    if open_brace < 0:
        raise ValueError(f"{path}: marker_for() body not found")

    depth = 0
    for i in range(open_brace, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[open_brace + 1 : i]
    raise ValueError(f"{path}: marker_for() body is unterminated")


def marker_for_patterns(path: Path) -> dict[str, str]:
    body = marker_for_body(read(path), path)
    return {name: ws63_skill_pattern(pattern) for name, pattern in CASE_RE.findall(body)}


def reference_marker_section(text: str) -> str:
    heading = "## `hil-smoke.sh` 当前检查的 grep 模式"
    start = text.find(heading)
    if start < 0:
        raise ValueError(f"{REFERENCE}: marker section heading not found")
    rest = text[start:]
    next_heading = rest.find("\n## ", len(heading))
    return rest if next_heading < 0 else rest[:next_heading]


def main() -> int:
    smoke_patterns = dict(CHECK_RE.findall(read(SMOKE)))
    reference_patterns = dict(REF_ROW_RE.findall(reference_marker_section(read(REFERENCE))))
    wrapper_patterns = marker_for_patterns(SKILL_WRAPPER)
    parity_patterns = marker_for_patterns(PARITY)

    issues: list[str] = []
    for name, pattern in sorted(smoke_patterns.items()):
        ref_pattern = reference_patterns.get(name)
        if ref_pattern != pattern:
            issues.append(
                f"{REFERENCE}: `{name}` pattern is {ref_pattern!r}, expected {pattern!r} from {SMOKE}"
            )

        wrapper_pattern = wrapper_patterns.get(name)
        if wrapper_pattern != pattern:
            issues.append(
                f"{SKILL_WRAPPER}: `{name}` pattern is {wrapper_pattern!r}, expected {pattern!r} from {SMOKE}"
            )

        parity_pattern = parity_patterns.get(name)
        if parity_pattern != pattern:
            issues.append(
                f"{PARITY}: `{name}` pattern is {parity_pattern!r}, expected {pattern!r} from {SMOKE}"
            )

    extra_ref = sorted(set(reference_patterns) - set(smoke_patterns))
    extra_wrapper = sorted(set(wrapper_patterns) - set(smoke_patterns))
    extra_parity = sorted(set(parity_patterns) - set(smoke_patterns))
    if extra_ref:
        issues.append(f"{REFERENCE}: extra marker rows not present in {SMOKE}: {', '.join(extra_ref)}")
    if extra_wrapper:
        issues.append(f"{SKILL_WRAPPER}: extra marker cases not present in {SMOKE}: {', '.join(extra_wrapper)}")
    if extra_parity:
        issues.append(f"{PARITY}: extra marker cases not present in {SMOKE}: {', '.join(extra_parity)}")

    if issues:
        print("HIL smoke marker drift found:")
        for issue in issues:
            print(f"  - {issue}")
        return 1

    print(f"HIL smoke markers in sync: {len(smoke_patterns)} examples")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
