#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Unit and negative tests for the executable FlashPlan evidence gate."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import subprocess
import tempfile
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
    def test_rejects_incomplete_and_overlapping_writes(self) -> None:
        for chunks in (
            [{"addr": 0x230000, "image_offset": 0, "len": 1}],
            [{"addr": 0x230000, "image_offset": 0, "len": 32}] * 2,
        ):
            plan, image = valid_fixture()
            plan["write_chunks"] = chunks
            with self.assertRaisesRegex(ValueError, "exactly once"):
                MODULE.validate_flash_plan(plan, image)

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


@unittest.skipUnless(os.environ.get("FLASH_PLAN_INTEGRATION") == "1", "requires pinned hisi-fwpkg CLI")
class ImageSemanticsTests(unittest.TestCase):
    def test_real_images_and_header_corruption(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for size in (16, 0xE1001):
                source, output = root / "body.bin", root / "image.img"
                source.write_bytes(bytes(range(256)) * (size // 256) + bytes(size % 256))
                plan = json.loads(subprocess.check_output(
                    ["hisi-fwpkg", "plan", str(source), "--chip", "ws63", "--image-output", str(output)],
                    text=True,
                ))
                image = output.read_bytes()
                MODULE.validate_flash_plan(plan, image)
                MODULE.validate_image_semantics(plan, image)
                header_end = plan["body_range"]["image_offset"]
                corrupted = bytes(header_end) + image[header_end:]
                with self.assertRaisesRegex(ValueError, "canonical"):
                    MODULE.validate_image_semantics(plan, corrupted)
                hash_offset = image.find(bytes(plan["code_area_hash"]), 0, header_end)
                self.assertGreaterEqual(hash_offset, 0)
                stale_header = bytearray(image)
                stale_header[hash_offset] ^= 1
                with self.assertRaisesRegex(ValueError, "canonical"):
                    MODULE.validate_image_semantics(plan, bytes(stale_header))

    def test_rejects_mismatched_chunk_address(self) -> None:
        plan, image = valid_fixture()
        plan["write_chunks"][0]["addr"] += 4
        with self.assertRaisesRegex(ValueError, "address does not match"):
            MODULE.validate_flash_plan(plan, image, 0x230000)


if __name__ == "__main__":
    unittest.main()
