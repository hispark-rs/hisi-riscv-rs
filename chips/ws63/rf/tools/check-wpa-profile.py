#!/usr/bin/env python3
"""Check that the SDK WPA component can be narrowed to the WPA2 profile."""

from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from pathlib import Path


SOURCE_RE = re.compile(r"\$\{ROOT_DIR\}([^\s)]+\.c)")


def cmake_block(text: str, name: str) -> str:
    match = re.search(rf"set\({name}\s+(.*?)\n\)", text, re.DOTALL)
    if not match:
        raise ValueError(f"missing set({name} ...) block")
    return match.group(1)


def cmake_sources(block: str) -> list[str]:
    sources: set[str] = set()
    for raw_line in block.splitlines():
        line = raw_line.split("#", 1)[0]
        sources.update(SOURCE_RE.findall(line))
    return sorted(sources)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument(
        "--profile",
        type=Path,
        default=Path(__file__).with_name("wpa2-personal-profile.toml"),
    )
    parser.add_argument("--format", choices=("text", "json"), default="text")
    args = parser.parse_args()

    profile = tomllib.loads(args.profile.read_text())
    component = args.sdk / profile["sdk_component"]
    text = component.read_text()
    sources = cmake_sources(cmake_block(text, "SOURCES"))
    defines = set(re.findall(r"^\s*([A-Z][A-Z0-9_]*(?:=[^\s)]+)?)\s*$", cmake_block(text, "PRIVATE_DEFINES"), re.MULTILINE))

    missing_defines = sorted(set(profile["required_defines"]) - defines)
    forbidden_defines = set(profile["forbidden_defines"])
    selected_defines = sorted(defines - forbidden_defines)
    fragments = tuple(profile["forbidden_source_fragments"])
    selected_sources = [source for source in sources if not any(fragment in source for fragment in fragments)]
    leaked_sources = [source for source in selected_sources if any(fragment in source for fragment in fragments)]

    result = {
        "profile": profile["name"],
        "sdk_component": str(component),
        "source_count_before": len(sources),
        "source_count_after": len(selected_sources),
        "define_count_after": len(selected_defines),
        "missing_required_defines": missing_defines,
        "forbidden_source_leaks": leaked_sources,
        "selected_sources": selected_sources,
        "selected_defines": selected_defines,
        "crypto": profile["crypto"],
    }
    if args.format == "json":
        print(json.dumps(result, indent=2, sort_keys=True))
    else:
        print(f"{result['profile']}: {len(sources)} -> {len(selected_sources)} sources")
        print(f"defines: {len(defines)} -> {len(selected_defines)}")
        for name in selected_sources:
            print(name)

    if missing_defines or leaked_sources:
        print("WPA profile validation failed", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
