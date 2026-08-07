#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Unit tests for the SLE S3 paired-board marker contract."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("ws63-sle-ssap-reset-matrix.py")
SPEC = importlib.util.spec_from_file_location("ws63_sle_ssap_matrix", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ClassifyTests(unittest.TestCase):
    def test_complete_data_lifecycle_passes(self) -> None:
        result = MODULE.classify(
            b"\n".join(MODULE.SERVER_MARKERS), b"\n".join(MODULE.CLIENT_MARKERS)
        )
        self.assertTrue(result["pass"])

    def test_missing_client_data_fails(self) -> None:
        result = MODULE.classify(
            b"\n".join(MODULE.SERVER_MARKERS),
            b"\n".join(
                marker
                for marker in MODULE.CLIENT_MARKERS
                if marker != b"RFDBG_SLE_S3_CLIENT_DATA_OK"
            ),
        )
        self.assertFalse(result["pass"])
        self.assertEqual(result["client_missing"], ["RFDBG_SLE_S3_CLIENT_DATA_OK"])

    def test_missing_service_discovery_fails(self) -> None:
        result = MODULE.classify(
            b"\n".join(MODULE.SERVER_MARKERS),
            b"\n".join(
                marker
                for marker in MODULE.CLIENT_MARKERS
                if marker != b"RFDBG_SLE_S3_CLIENT_SERVICE_OK"
            ),
        )
        self.assertFalse(result["pass"])
        self.assertEqual(
            result["client_missing"], ["RFDBG_SLE_S3_CLIENT_SERVICE_OK"]
        )

    def test_command_failure_fails_closed(self) -> None:
        result = MODULE.classify(
            b"\n".join(MODULE.SERVER_MARKERS),
            b"\n".join((*MODULE.CLIENT_MARKERS, MODULE.FAILURE_MARKERS[-1])),
        )
        self.assertFalse(result["pass"])
        self.assertEqual(result["failure_markers"], ["RFDBG_SLE_S3_CLIENT_EVENT_DROP"])


if __name__ == "__main__":
    unittest.main()
