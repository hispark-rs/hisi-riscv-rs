#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Print a FlashPlan base address in probe-rs-compatible hexadecimal form."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


parser = argparse.ArgumentParser()
parser.add_argument("plan", type=Path)
args = parser.parse_args()

with args.plan.open(encoding="utf-8") as plan_file:
    plan = json.load(plan_file)

print(f"0x{plan['base_addr']:08X}")
