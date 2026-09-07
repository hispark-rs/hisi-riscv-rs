#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Unit and negative tests for the executable FlashPlan evidence gate."""

from __future__ import annotations

import hashlib
import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("check-flash-plan.py")
SPEC = importlib.util.spec_from_file_location("check_flash_plan", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {SCRIPT}")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def valid_fixture() -> tuple[dict, bytes]:
    image = bytes(range(32))
    body = image[8:24]
    plan = {
        "base_addr": 0x230000,
        "image_len": len(image),
        "body_range": {
            "image_offset": 8,
            "flash_addr": 0x230008,
            "len": len(body),
        },
        "code_area_len": len(body),
        "code_area_hash": list(hashlib.sha256(body).digest()),
        "erase_range": {"start": 0x230000, "len": len(image)},
        "write_chunks": [
            {"addr": 0x230000, "image_offset": 0, "len": len(image)}
        ],
    }
    return plan, image


class FlashPlanTests(unittest.TestCase):
    def test_accepts_bound_image_and_body_hash(self) -> None:
        plan, image = valid_fixture()
        MODULE.validate_flash_plan(plan, image, 0x230000)

    def test_rejects_body_hash_mismatch(self) -> None:
        plan, image = valid_fixture()
        corrupted = image[:12] + b"\xff" + image[13:]
        with self.assertRaisesRegex(ValueError, "code_area_hash"):
            MODULE.validate_flash_plan(plan, corrupted, 0x230000)

    def test_rejects_write_outside_erase_range(self) -> None:
        plan, image = valid_fixture()
        plan["erase_range"]["len"] -= 1
        with self.assertRaisesRegex(ValueError, "complete planned image"):
            MODULE.validate_flash_plan(plan, image, 0x230000)

    def test_rejects_mismatched_chunk_address(self) -> None:
        plan, image = valid_fixture()
        plan["write_chunks"][0]["addr"] += 4
        with self.assertRaisesRegex(ValueError, "address does not match"):
            MODULE.validate_flash_plan(plan, image, 0x230000)


if __name__ == "__main__":
    unittest.main()
