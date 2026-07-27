#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyserial"]
# ///
"""Host tests for the unchanged-image WS63 connectivity reset matrix."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("ws63-connectivity-reset-matrix.py")
SPEC = importlib.util.spec_from_file_location("ws63_connectivity_reset_matrix", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MATRIX = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MATRIX)


class ClassifyTests(unittest.TestCase):
    def test_pure_wpa3_gate_accepts_matching_success(self) -> None:
        log = b"\n".join(
            (
                b"W2E_AP_SECURITY mode=pure-wpa3",
                b"W2E_WPA3_CONNECT_OK pmf=required",
            )
        )
        self.assertEqual(MATRIX.classify(log, "rust", "connect", "pure-wpa3"), "pass")

    def test_pure_wpa3_gate_rejects_transition_success(self) -> None:
        log = b"\n".join(
            (
                b"W2E_AP_SECURITY mode=transition",
                b"W2E_WPA3_CONNECT_OK pmf=required",
            )
        )
        self.assertEqual(
            MATRIX.classify(log, "rust", "connect", "pure-wpa3"),
            "wrong_ap_mode",
        )

    def test_missing_mode_does_not_hide_connect_error(self) -> None:
        log = b"W2E_WPA3_CONNECT_ERR code=0x1"
        self.assertEqual(
            MATRIX.classify(log, "rust", "connect", "pure-wpa3"),
            "connect_error",
        )

    def test_pure_wpa3_gate_rejects_success_without_mode_evidence(self) -> None:
        log = b"W2E_WPA3_CONNECT_OK pmf=required"
        self.assertEqual(
            MATRIX.classify(log, "rust", "connect", "pure-wpa3"),
            "missing_ap_mode",
        )

    def test_transition_gate_accepts_matching_a5b_success(self) -> None:
        log = b"\n".join(
            (
                b"W2E_AP_SECURITY mode=transition",
                b"RFDBG_A5B_CONNECT_PROFILE_OK",
            )
        )
        self.assertEqual(MATRIX.classify(log, "rust", "connect", "transition"), "pass")

    def test_transition_gate_rejects_a5b_success_without_mode_evidence(self) -> None:
        log = b"RFDBG_A5B_CONNECT_PROFILE_OK"
        self.assertEqual(
            MATRIX.classify(log, "rust", "connect", "transition"),
            "missing_ap_mode",
        )

    def test_terminal_capture_can_finish_while_uart_is_silent(self) -> None:
        self.assertFalse(MATRIX.post_terminal_elapsed(None, 2.0, 100.0))
        self.assertFalse(MATRIX.post_terminal_elapsed(10.0, 2.0, 11.999))
        self.assertTrue(MATRIX.post_terminal_elapsed(10.0, 2.0, 12.0))


if __name__ == "__main__":
    unittest.main()
