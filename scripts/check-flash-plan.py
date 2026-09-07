#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Validate the executable FlashPlan contract used by tutorials and HIL."""

from __future__ import annotations

import argparse
import hashlib
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

def require_range(value: object, name: str) -> tuple[int, int]:
    if not isinstance(value, dict):
        raise ValueError(f"{name} must be an object")
    start_key = "image_offset" if name == "body_range" else "start"
    start = value.get(start_key)
    length = value.get("len")
    if not isinstance(start, int) or not isinstance(length, int):
        raise ValueError(f"{name} must contain integer {start_key} and len")
    if start < 0 or length <= 0:
        raise ValueError(f"{name} must be non-negative and non-empty")
    return start, length


def validate_flash_plan(
    plan: dict,
    image: bytes,
    expected_base_address: int | None = None,
) -> None:
    missing = [key for key in REQUIRED_KEYS if key not in plan]
    if missing:
        raise ValueError(f"missing plan keys: {missing}")

    base_addr = plan["base_addr"]
    if not isinstance(base_addr, int) or base_addr < 0:
        raise ValueError("base_addr must be a non-negative integer")
    if expected_base_address is not None and base_addr != expected_base_address:
        raise ValueError(
            f"unexpected base_addr: {base_addr:#x}, expected {expected_base_address:#x}"
        )
    if plan["image_len"] != len(image):
        raise ValueError("plan image_len does not match image file size")

    body_offset, body_len = require_range(plan["body_range"], "body_range")
    body_end = body_offset + body_len
    if body_end > len(image):
        raise ValueError("body_range extends beyond the image")
    if plan["body_range"].get("flash_addr") != base_addr + body_offset:
        raise ValueError("body_range flash_addr does not match base_addr + image_offset")
    if plan["code_area_len"] != body_len:
        raise ValueError("code_area_len does not match body_range.len")

    expected_hash = plan["code_area_hash"]
    if (
        not isinstance(expected_hash, list)
        or len(expected_hash) != 32
        or any(not isinstance(byte, int) or not 0 <= byte <= 255 for byte in expected_hash)
    ):
        raise ValueError("code_area_hash must contain exactly 32 bytes")
    actual_hash = hashlib.sha256(image[body_offset:body_end]).digest()
    if actual_hash != bytes(expected_hash):
        raise ValueError("code_area_hash does not match the planned image body")

    erase_start, erase_len = require_range(plan["erase_range"], "erase_range")
    erase_end = erase_start + erase_len
    if erase_start > base_addr or erase_end < base_addr + len(image):
        raise ValueError("erase_range does not cover the complete planned image")

    chunks = plan["write_chunks"]
    if not isinstance(chunks, list) or not chunks:
        raise ValueError("write_chunks must not be empty")
    for index, chunk in enumerate(chunks):
        if not isinstance(chunk, dict):
            raise ValueError(f"write_chunks[{index}] must be an object")
        addr = chunk.get("addr")
        offset = chunk.get("image_offset")
        length = chunk.get("len")
        if not all(isinstance(value, int) for value in (addr, offset, length)):
            raise ValueError(f"write_chunks[{index}] fields must be integers")
        if offset < 0 or length <= 0 or offset + length > len(image):
            raise ValueError(f"write_chunks[{index}] extends beyond the image")
        if addr != base_addr + offset:
            raise ValueError(f"write_chunks[{index}] address does not match image offset")
        if addr < erase_start or addr + length > erase_end:
            raise ValueError(f"write_chunks[{index}] extends beyond erase_range")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("plan", type=Path)
    parser.add_argument("image", type=Path)
    parser.add_argument("--base-address", type=lambda value: int(value, 0))
    args = parser.parse_args()

    with args.plan.open(encoding="utf-8") as plan_file:
        plan = json.load(plan_file)
    image = args.image.read_bytes()
    try:
        validate_flash_plan(plan, image, args.base_address)
    except ValueError as error:
        raise SystemExit(str(error)) from error

    print(
        f"flash plan OK: {args.image} ({len(image)} bytes, "
        f"body sha256={bytes(plan['code_area_hash']).hex()})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
