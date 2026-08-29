#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyserial"]
# ///
"""Host tests for paired WS63 coexistence marker contracts."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("ws63-coexistence-reset-matrix.py")
SPEC = importlib.util.spec_from_file_location("ws63_coexistence_matrix", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MATRIX = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MATRIX)


def complete_log(contract: str, role: str) -> bytes:
    markers = list(MATRIX.required_markers(contract, role))
    if contract == "ble-activity" and role == "ble":
        markers.extend([MATRIX.BLE_SCAN_MARKER] * MATRIX.BLE_ACTIVITY_SCAN_COUNT)
    return b"\n".join(markers)


class ClassifyTests(unittest.TestCase):
    def test_shared_init_contract_is_backward_compatible(self) -> None:
        for role in ("ble", "sle"):
            with self.subTest(role=role):
                result = MATRIX.classify(
                    "shared-init", role, complete_log("shared-init", role)
                )
                self.assertTrue(result["pass"])

    def test_ble_activity_requires_three_scans(self) -> None:
        result = MATRIX.classify(
            "ble-activity", "ble", complete_log("ble-activity", "ble")
        )
        self.assertTrue(result["pass"])
        self.assertEqual(result["wifi_scan_ok_count"], 3)

    def test_ble_activity_fails_when_scan_count_is_short(self) -> None:
        payload = complete_log("ble-activity", "ble").replace(
            MATRIX.BLE_SCAN_MARKER + b"\n", b"", 1
        )
        result = MATRIX.classify("ble-activity", "ble", payload)
        self.assertFalse(result["pass"])
        self.assertEqual(result["wifi_scan_ok_count"], 2)
        self.assertIn("observed 2", result["missing"][-1])

    def test_ble_activity_fails_closed_on_event_drop(self) -> None:
        payload = complete_log("ble-activity", "ble")
        payload += b"\nRFDBG_COEX_BLE_EVENT_DROP"
        result = MATRIX.classify("ble-activity", "ble", payload)
        self.assertFalse(result["pass"])
        self.assertEqual(
            result["failure_markers"], ["RFDBG_COEX_BLE_EVENT_DROP"]
        )

    def test_sle_peer_remains_shared_init_control(self) -> None:
        result = MATRIX.classify(
            "ble-activity", "sle", complete_log("ble-activity", "sle")
        )
        self.assertTrue(result["pass"])
        self.assertEqual(result["wifi_scan_ok_count"], 0)


if __name__ == "__main__":
    unittest.main()
