#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Host tests for the paired-board WS63 BLE discovery matrix."""

import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("ws63-ble-discovery-reset-matrix.py")
SPEC = importlib.util.spec_from_file_location("ws63_ble_discovery_matrix", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MATRIX = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MATRIX)


def complete_log(*extra: bytes) -> bytes:
    return b"\n".join((*MATRIX.REQUIRED_BOOT_MARKERS, MATRIX.MATCH_MARKER, *extra))


class ClassifyTests(unittest.TestCase):
    def test_complete_paired_discovery_passes(self) -> None:
        result = MATRIX.classify(complete_log(), complete_log())
        self.assertTrue(result["pass"])
        self.assertTrue(result["target_match"])
        self.assertTrue(result["peer_match"])

    def test_missing_operation_marker_fails(self) -> None:
        target = b"\n".join(MATRIX.REQUIRED_BOOT_MARKERS[:-1])
        result = MATRIX.classify(target, complete_log())
        self.assertFalse(result["pass"])
        self.assertIn("RFDBG_BLE_B2_ADV_OK", result["target_missing"])

    def test_missing_callback_fails(self) -> None:
        result = MATRIX.classify(
            complete_log(b"RFDBG_MISSING_ROM_CALLBACK ra=0x00123456"),
            complete_log(),
        )
        self.assertFalse(result["pass"])
        self.assertIn("RFDBG_MISSING_ROM_CALLBACK", result["failure_markers"])

    def test_event_drop_fails(self) -> None:
        result = MATRIX.classify(
            complete_log(), complete_log(b"RFDBG_BLE_B2_EVENT_DROP")
        )
        self.assertFalse(result["pass"])
        self.assertIn("RFDBG_BLE_B2_EVENT_DROP", result["failure_markers"])


if __name__ == "__main__":
    unittest.main()
