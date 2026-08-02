#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyserial"]
# ///
"""Host tests for deterministic J-Link selection in nRST capture."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("nrst-and-capture.py")
SPEC = importlib.util.spec_from_file_location("nrst_and_capture", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class JLinkSelectionTests(unittest.TestCase):
    def test_single_probe_uses_legacy_unselected_invocation(self) -> None:
        self.assertEqual(
            MODULE.jlink_argv("JLinkExe", "/tmp/command.jlink", None),
            [
                "JLinkExe",
                "-NoGui",
                "1",
                "-CommandFile",
                "/tmp/command.jlink",
            ],
        )

    def test_multi_probe_selects_exact_emulator_serial(self) -> None:
        self.assertEqual(
            MODULE.jlink_argv("JLinkExe", "/tmp/command.jlink", "12345678"),
            [
                "JLinkExe",
                "-NoGui",
                "1",
                "-SelectEmuBySN",
                "12345678",
                "-CommandFile",
                "/tmp/command.jlink",
            ],
        )

    def test_serial_rejects_shell_or_option_injection(self) -> None:
        for invalid in ("", "23-12", "--help", "１２３"):
            with self.subTest(invalid=invalid):
                with self.assertRaises(ValueError):
                    MODULE.jlink_argv("JLinkExe", "/tmp/command.jlink", invalid)


if __name__ == "__main__":
    unittest.main()
