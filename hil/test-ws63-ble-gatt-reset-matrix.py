#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("ws63-ble-gatt-reset-matrix.py")
SPEC = importlib.util.spec_from_file_location("ws63_ble_gatt_matrix", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ClassificationTests(unittest.TestCase):
    def test_complete_contract_passes(self) -> None:
        server = b"\n".join(MODULE.SERVER_MARKERS)
        client = b"\n".join(MODULE.CLIENT_MARKERS)
        result = MODULE.classify(server, client)
        self.assertTrue(result["pass"])
        self.assertEqual(result["failure_markers"], [])

    def test_missing_indication_confirmation_fails(self) -> None:
        server = b"\n".join(
            marker
            for marker in MODULE.SERVER_MARKERS
            if marker != b"RFDBG_BLE_B3_INDICATE_CONFIRMED"
        )
        client = b"\n".join(MODULE.CLIENT_MARKERS)
        result = MODULE.classify(server, client)
        self.assertFalse(result["pass"])
        self.assertIn(
            "RFDBG_BLE_B3_INDICATE_CONFIRMED", result["server_missing"]
        )

    def test_event_drop_fails_closed(self) -> None:
        server = b"\n".join(MODULE.SERVER_MARKERS)
        client = b"\n".join(MODULE.CLIENT_MARKERS) + b"\nRFDBG_BLE_B3_EVENT_DROP"
        result = MODULE.classify(server, client)
        self.assertFalse(result["pass"])
        self.assertIn("RFDBG_BLE_B3_EVENT_DROP", result["failure_markers"])


if __name__ == "__main__":
    unittest.main()
