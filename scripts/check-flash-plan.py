#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Validate the executable FlashPlan contract used by tutorials and HIL."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


REQUIRED_KEYS = (
    "base_addr",
    "image_len",
    "body_range",
    "code_area_len",
    "code_area_hash",
    "erase_range",
    "write_chunks",
)

parser = argparse.ArgumentParser()
parser.add_argument("plan", type=Path)
parser.add_argument("image", type=Path)
parser.add_argument("--base-address", type=lambda value: int(value, 0))
args = parser.parse_args()

with args.plan.open(encoding="utf-8") as plan_file:
    plan = json.load(plan_file)

missing = [key for key in REQUIRED_KEYS if key not in plan]
if missing:
    raise SystemExit(f"missing plan keys: {missing}")
if args.base_address is not None and plan["base_addr"] != args.base_address:
    raise SystemExit(
        f"unexpected base_addr: {plan['base_addr']:#x}, expected {args.base_address:#x}"
    )
if plan["image_len"] != args.image.stat().st_size:
    raise SystemExit("plan image_len does not match image file size")
if not plan["write_chunks"]:
    raise SystemExit("write_chunks must not be empty")

print(f"flash plan OK: {args.image} ({plan['image_len']} bytes)")
