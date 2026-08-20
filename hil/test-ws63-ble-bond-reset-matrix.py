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
    @staticmethod
    def record(run: int, restored: bool, passed: bool = True) -> dict[str, object]:
        return {"run": run, "restored_contract": restored, "pass": passed}

    def test_complete_contract_passes(self) -> None:
        peripheral = b"\n".join(
            (MATRIX.PERIPHERAL_STARTUP_MARKERS[0], *MATRIX.PERIPHERAL_MARKERS)
        )
        central = b"\n".join(
            (MATRIX.CENTRAL_STARTUP_MARKERS[0], *MATRIX.CENTRAL_MARKERS)
        )
        result = MATRIX.classify(peripheral, central)
        self.assertTrue(result["pass"])
        self.assertEqual(result["failure_markers"], [])

    def test_restored_contract_passes_without_repair_markers(self) -> None:
        peripheral = b"\n".join(
            (
                MATRIX.PERIPHERAL_STARTUP_MARKERS[1],
                *MATRIX.PERIPHERAL_RESTORED_MARKERS,
            )
        )
        central = b"\n".join(
            (MATRIX.CENTRAL_STARTUP_MARKERS[1], *MATRIX.CENTRAL_RESTORED_MARKERS)
        )
        result = MATRIX.classify(peripheral, central)
        self.assertTrue(result["pass"])
        self.assertTrue(result["restored_contract"])

    def test_restored_contract_requires_active_state_proof(self) -> None:
        peripheral = b"\n".join(
            (
                MATRIX.PERIPHERAL_STARTUP_MARKERS[1],
                *MATRIX.PERIPHERAL_RESTORED_MARKERS[:-2],
                MATRIX.PERIPHERAL_RESTORED_MARKERS[-1],
            )
        )
        central = b"\n".join(
            (MATRIX.CENTRAL_STARTUP_MARKERS[1], *MATRIX.CENTRAL_RESTORED_MARKERS)
        )
        result = MATRIX.classify(peripheral, central)
        self.assertFalse(result["pass"])
        self.assertIn(
            "RFDBG_RADIO_U5_BLE_PERIPHERAL_RESTORED_ACTIVE",
            result["peripheral_missing"],
        )

    def test_removal_contract_requires_completed_remove_command(self) -> None:
        peripheral = b"\n".join(
            (MATRIX.PERIPHERAL_STARTUP_MARKERS[1], *MATRIX.PERIPHERAL_REMOVAL_MARKERS)
        )
        central = b"\n".join(
            (MATRIX.CENTRAL_STARTUP_MARKERS[1], *MATRIX.CENTRAL_REMOVAL_MARKERS)
        )
        self.assertTrue(
            MATRIX.classify(peripheral, central, expect_removal=True)["pass"]
        )

        peripheral = peripheral.replace(
            MATRIX.PERIPHERAL_REMOVAL_MARKERS[-2], b""
        )
        result = MATRIX.classify(peripheral, central, expect_removal=True)
        self.assertFalse(result["pass"])
        self.assertIn(
            "RFDBG_RADIO_U5_BLE_PERIPHERAL_BOND_REMOVED",
            result["peripheral_missing"],
        )

    def test_nv_write_failure_fails_closed(self) -> None:
        peripheral = b"\n".join(
            (
                MATRIX.PERIPHERAL_STARTUP_MARKERS[1],
                *MATRIX.PERIPHERAL_REMOVAL_MARKERS,
                b"RFDBG_NV_WRITE_ERR",
            )
        )
        central = b"\n".join(
            (MATRIX.CENTRAL_STARTUP_MARKERS[1], *MATRIX.CENTRAL_REMOVAL_MARKERS)
        )
        result = MATRIX.classify(peripheral, central, expect_removal=True)
        self.assertFalse(result["pass"])
        self.assertIn("RFDBG_NV_WRITE_ERR", result["failure_markers"])

    def test_mixed_restore_state_fails_closed(self) -> None:
        peripheral = b"\n".join(
            (
                MATRIX.PERIPHERAL_STARTUP_MARKERS[1],
                *MATRIX.PERIPHERAL_RESTORED_MARKERS,
            )
        )
        central = b"\n".join(
            (MATRIX.CENTRAL_STARTUP_MARKERS[0], *MATRIX.CENTRAL_MARKERS)
        )
        result = MATRIX.classify(peripheral, central)
        self.assertFalse(result["pass"])
        self.assertTrue(result["restore_mismatch"])

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

    def test_lifecycle_failure_fails_closed(self) -> None:
        peripheral = b"\n".join(
            (MATRIX.PERIPHERAL_STARTUP_MARKERS[0], *MATRIX.PERIPHERAL_MARKERS)
        )
        central = b"\n".join(
            (MATRIX.CENTRAL_STARTUP_MARKERS[0], *MATRIX.CENTRAL_MARKERS)
        )
        central += b"\nRFDBG_RADIO_U5_BLE_SCAN_STOP_ERR"
        result = MATRIX.classify(peripheral, central)
        self.assertFalse(result["pass"])
        self.assertIn(
            "RFDBG_RADIO_U5_BLE_SCAN_STOP_ERR", result["failure_markers"]
        )

    def test_swapped_uart_roles_are_reported_directly(self) -> None:
        peripheral = b"\n".join(
            (MATRIX.CENTRAL_STARTUP_MARKERS[0], *MATRIX.CENTRAL_MARKERS)
        )
        central = b"\n".join(
            (MATRIX.PERIPHERAL_STARTUP_MARKERS[0], *MATRIX.PERIPHERAL_MARKERS)
        )
        result = MATRIX.classify(peripheral, central)
        self.assertFalse(result["pass"])
        self.assertTrue(result["role_mismatch"])

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

    def test_persisted_uart_logs_redact_pairing_passkeys(self) -> None:
        raw = b"RFDBG_RADIO_U5_BLE_PERIPHERAL_PASSKEY_DISPLAY=123456\r\n"
        redacted = MATRIX.redact_passkeys(raw)
        self.assertEqual(redacted, MATRIX.PASSKEY_REDACTED + b"\r\n")
        self.assertNotIn(b"123456", redacted)

    def test_fresh_then_restored_proves_persistence(self) -> None:
        result = MATRIX.validate_persistence(
            [self.record(1, False), self.record(2, True), self.record(3, True)]
        )
        self.assertTrue(result["proven"])

    def test_single_fresh_run_does_not_prove_persistence(self) -> None:
        result = MATRIX.validate_persistence([self.record(1, False)])
        self.assertFalse(result["proven"])
        self.assertIn("subsequent reset", result["errors"][0])

    def test_repeated_fresh_pairing_fails_persistence(self) -> None:
        result = MATRIX.validate_persistence(
            [self.record(1, False), self.record(2, False), self.record(3, False)]
        )
        self.assertFalse(result["proven"])
        self.assertEqual(len(result["errors"]), 2)

    def test_restored_bond_must_not_regress_to_empty(self) -> None:
        result = MATRIX.validate_persistence(
            [self.record(1, True), self.record(2, False)]
        )
        self.assertFalse(result["proven"])
        self.assertIn("lost a bond", result["errors"][0])

    def test_removal_lifecycle_alternates_restored_and_empty(self) -> None:
        result = MATRIX.validate_persistence(
            [
                self.record(1, True),
                self.record(2, False),
                self.record(3, True),
            ],
            expect_removal=True,
        )
        self.assertTrue(result["proven"])

    def test_removal_lifecycle_rejects_a_stale_restored_bond(self) -> None:
        result = MATRIX.validate_persistence(
            [self.record(1, True), self.record(2, True)],
            expect_removal=True,
        )
        self.assertFalse(result["proven"])
        self.assertIn("was not empty", result["errors"][0])


if __name__ == "__main__":
    unittest.main()
