#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""List and lightly validate hisi-hal embedded-test HIL registrations."""

from __future__ import annotations

import argparse
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]
HARNESS = ROOT / "crates/hisi-hal/tests/hil.rs"
HELPERS = ROOT / "crates/hisi-hal/tests/hil"


MOD_RE = re.compile(r'#\[path\s*=\s*"hil/([^"]+\.rs)"\]\s*\nmod\s+([A-Za-z0-9_]+);')
TEST_RE = re.compile(
    r'(?P<attrs>(?:\s*#\[[^\]]+\]\s*)*)\s*fn\s+(?P<name>[A-Za-z0-9_]+)\s*\([^)]*\)\s*\{(?P<body>.*?)\n\s*\}',
    re.S,
)
CALL_RE = re.compile(r"crate::([A-Za-z0-9_]+)::([A-Za-z0-9_]+)\s*\(")
HELPER_FN_RE = re.compile(r"pub\(crate\)\s+fn\s+([A-Za-z0-9_]+)\s*\(")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--strict", action="store_true", help="return non-zero on missing modules/helpers")
    args = parser.parse_args()

    text = HARNESS.read_text()
    modules = {mod: file for file, mod in MOD_RE.findall(text)}
    tests = []
    issues = []

    for match in TEST_RE.finditer(text):
        attrs = match.group("attrs")
        if "#[test]" not in attrs:
            continue
        name = match.group("name")
        body = match.group("body")
        calls = CALL_RE.findall(body)
        cfgs = " ".join(re.findall(r"#\[cfg[^\]]+\]", attrs))
        tests.append((name, cfgs, calls))
        for mod, helper in calls:
            helper_path = HELPERS / modules.get(mod, "")
            if mod not in modules:
                issues.append(f"{name}: calls crate::{mod}::{helper} but module is not registered")
                continue
            if not helper_path.exists():
                issues.append(f"{name}: registered module file missing: {helper_path}")
                continue
            helper_text = helper_path.read_text()
            helpers = set(HELPER_FN_RE.findall(helper_text))
            if helper not in helpers:
                issues.append(f"{name}: helper {helper} not found in {helper_path}")

    print(f"Harness: {HARNESS}")
    print(f"Registered modules: {len(modules)}")
    for mod, file in sorted(modules.items()):
        print(f"  {mod:14s} {file}")

    print(f"\nEmbedded-test wrappers: {len(tests)}")
    for name, cfgs, calls in tests:
        call_text = ", ".join(f"{m}::{f}" for m, f in calls) or "(inline)"
        cfg_text = f" [{cfgs}]" if cfgs else ""
        print(f"  {name}{cfg_text} -> {call_text}")

    if issues:
        print("\nIssues:")
        for issue in issues:
            print(f"  - {issue}")
    return 1 if args.strict and issues else 0


if __name__ == "__main__":
    raise SystemExit(main())
