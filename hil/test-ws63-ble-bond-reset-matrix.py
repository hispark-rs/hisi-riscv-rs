#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Host tests for the paired-board WS63 BLE bond matrix."""

import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("ws63-ble-bond-reset-matrix.py")
SPEC = importlib.util.spec_from_file_location("ws63_ble_bond_matrix", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MATRIX = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MATRIX)


class ClassificationTests(unittest.TestCase):
    def test_complete_contract_passes(self) -> None:
        peripheral = b"\n".join(
            (MATRIX.PERIPHERAL_STARTUP_MARKERS[1], *MATRIX.PERIPHERAL_MARKERS)
        )
        central = b"\n".join(
            (MATRIX.CENTRAL_STARTUP_MARKERS[1], *MATRIX.CENTRAL_MARKERS)
        )
        result = MATRIX.classify(peripheral, central)
        self.assertTrue(result["pass"])
        self.assertEqual(result["failure_markers"], [])

    def test_missing_bond_observer_fails(self) -> None:
        peripheral = b"\n".join(
            (MATRIX.PERIPHERAL_STARTUP_MARKERS[0], *MATRIX.PERIPHERAL_MARKERS[:-2])
        )
        central = b"\n".join(
            (MATRIX.CENTRAL_STARTUP_MARKERS[0], *MATRIX.CENTRAL_MARKERS)
        )
        result = MATRIX.classify(peripheral, central)
        self.assertFalse(result["pass"])
        self.assertIn(
            "RFDBG_RADIO_U5_BLE_PERIPHERAL_BOND_OBSERVED",
            result["peripheral_missing"],
        )

    def test_conservation_failure_fails_closed(self) -> None:
        peripheral = b"\n".join(
            (MATRIX.PERIPHERAL_STARTUP_MARKERS[0], *MATRIX.PERIPHERAL_MARKERS)
        )
        central = b"\n".join(
            (MATRIX.CENTRAL_STARTUP_MARKERS[0], *MATRIX.CENTRAL_MARKERS)
        )
        central += b"\nRFDBG_RADIO_U5_BLE_BOND_CONSERVATION_ERR"
        result = MATRIX.classify(peripheral, central)
        self.assertFalse(result["pass"])
        self.assertIn(
            "RFDBG_RADIO_U5_BLE_BOND_CONSERVATION_ERR",
            result["failure_markers"],
        )

    def test_missing_or_ambiguous_restore_state_fails(self) -> None:
        peripheral = b"\n".join(MATRIX.PERIPHERAL_MARKERS)
        central = b"\n".join(
            (MATRIX.CENTRAL_STARTUP_MARKERS[0], *MATRIX.CENTRAL_MARKERS)
        )
        self.assertFalse(MATRIX.classify(peripheral, central)["pass"])

        peripheral = b"\n".join(
            (*MATRIX.PERIPHERAL_STARTUP_MARKERS, *MATRIX.PERIPHERAL_MARKERS)
        )
        self.assertFalse(MATRIX.classify(peripheral, central)["pass"])

    def test_sha256_binds_exact_elf(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "firmware.elf"
            path.write_bytes(b"u5")
            self.assertEqual(
                MATRIX.sha256(path),
                "5850a03e801ffb108da1160e3373979443004b9e670addf33000dca9045fa413",
            )


if __name__ == "__main__":
    unittest.main()
