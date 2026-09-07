#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Negative tests for the release image-format drift gate."""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("check-image-format-truth.py")
SPEC = importlib.util.spec_from_file_location("check_image_format_truth", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {SCRIPT}")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ReleaseImageTruthTests(unittest.TestCase):
    def setUp(self) -> None:
        self.release = MODULE.RELEASE_WORKFLOW.read_text(encoding="utf-8")

    def test_current_release_workflow_satisfies_contract(self) -> None:
        self.assertEqual(MODULE.release_workflow_errors(self.release), [])

    def test_missing_flash_plan_is_rejected(self) -> None:
        broken = self.release.replace("scripts/release-bundle.py prepare", "removed-plan-command")
        self.assertIn(
            "missing FlashPlan image generation",
            MODULE.release_workflow_errors(broken),
        )

    def test_raw_objcopy_release_path_is_rejected(self) -> None:
        broken = self.release + "\nrust-objcopy -O binary blinky blinky.bin\n"
        errors = MODULE.release_workflow_errors(broken)
        self.assertTrue(any("raw objcopy release path" in error for error in errors))
        self.assertTrue(any("raw binary release conversion" in error for error in errors))
        self.assertTrue(any("ambiguous raw binary" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
