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
from unittest import mock


SCRIPT = Path(__file__).with_name("nrst-and-capture.py")
SPEC = importlib.util.spec_from_file_location("nrst_and_capture", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
JLINK_SCRIPT = Path(__file__).with_name("jlink_nrst.py")
JLINK_SPEC = importlib.util.spec_from_file_location("jlink_nrst_test", JLINK_SCRIPT)
assert JLINK_SPEC is not None and JLINK_SPEC.loader is not None
JLINK = importlib.util.module_from_spec(JLINK_SPEC)
JLINK_SPEC.loader.exec_module(JLINK)


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

    def test_interactive_selection_uses_exact_emulator_serial(self) -> None:
        self.assertEqual(
            JLINK.interactive_jlink_argv("JLinkExe", "12345678"),
            ["JLinkExe", "-NoGui", "1", "-SelectEmuBySN", "12345678"],
        )

    @mock.patch.object(JLINK.time, "sleep")
    @mock.patch.object(JLINK.subprocess, "Popen")
    def test_held_reset_releases_on_body_failure(self, popen, _sleep) -> None:
        process = popen.return_value
        process.poll.return_value = None
        process.stdin = mock.Mock()

        with self.assertRaisesRegex(RuntimeError, "body failed"):
            with JLINK.held_nrst("JLinkExe", "12345678"):
                raise RuntimeError("body failed")

        process.stdin.write.assert_called_once_with("SetRESET\nsleep 200\n")
        process.stdin.flush.assert_called_once_with()
        process.communicate.assert_called_once_with(
            "ClrRESET\nsleep 200\nq\n", timeout=5
        )


if __name__ == "__main__":
    unittest.main()
