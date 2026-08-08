#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Host tests for the paired-board hisi-rf U2/U3 marker contract."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("ws63-radio-u2-reset-matrix.py")
SPEC = importlib.util.spec_from_file_location("ws63_radio_u2_matrix", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MATRIX = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MATRIX)


class ClassifyTests(unittest.TestCase):
    def complete_logs(self, protocol: str) -> tuple[bytes, bytes]:
        contract = MATRIX.PROTOCOLS[protocol]
        source = b"\n".join((MATRIX.COMMON_MARKER, contract["source_marker"]))
        observer = b"\n".join((MATRIX.COMMON_MARKER, contract["observer_marker"]))
        return source, observer

    def test_ble_lifecycle_passes(self) -> None:
        source, observer = self.complete_logs("ble")
        self.assertTrue(MATRIX.classify("ble", source, observer)["pass"])

    def test_sle_lifecycle_passes(self) -> None:
        source, observer = self.complete_logs("sle")
        self.assertTrue(MATRIX.classify("sle", source, observer)["pass"])

    def test_u3_typed_database_contracts_pass(self) -> None:
        for protocol in ("u3-ble", "u3-sle"):
            with self.subTest(protocol=protocol):
                source, observer = self.complete_logs(protocol)
                self.assertTrue(MATRIX.classify(protocol, source, observer)["pass"])

    def test_u3_inherits_u2_backend_failure_markers(self) -> None:
        source, observer = self.complete_logs("u3-ble")
        observer += b"\nRFDBG_RADIO_U2_BLE_EVENT_DROP"
        result = MATRIX.classify("u3-ble", source, observer)
        self.assertFalse(result["pass"])
        self.assertEqual(
            result["failure_markers"], ["RFDBG_RADIO_U2_BLE_EVENT_DROP"]
        )

    def test_missing_observer_marker_fails(self) -> None:
        source, _ = self.complete_logs("ble")
        result = MATRIX.classify("ble", source, MATRIX.COMMON_MARKER)
        self.assertFalse(result["pass"])
        self.assertEqual(
            result["observer_missing"], ["RFDBG_RADIO_U2_BLE_SCAN_OK"]
        )

    def test_protocol_failure_marker_fails_closed(self) -> None:
        source, observer = self.complete_logs("sle")
        observer += b"\nRFDBG_RADIO_U2_SLE_EVENT_DROP"
        result = MATRIX.classify("sle", source, observer)
        self.assertFalse(result["pass"])
        self.assertEqual(
            result["failure_markers"], ["RFDBG_RADIO_U2_SLE_EVENT_DROP"]
        )

    def test_common_failure_marker_fails_closed(self) -> None:
        source, observer = self.complete_logs("ble")
        source += b"\nRFDBG_MISSING_ROM_CALLBACK"
        result = MATRIX.classify("ble", source, observer)
        self.assertFalse(result["pass"])
        self.assertEqual(result["failure_markers"], ["RFDBG_MISSING_ROM_CALLBACK"])


if __name__ == "__main__":
    unittest.main()
