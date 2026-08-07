#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Unit tests for the SLE S2 paired-board marker contract."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("ws63-sle-connect-reset-matrix.py")
SPEC = importlib.util.spec_from_file_location("ws63_sle_connect_matrix", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ClassifyTests(unittest.TestCase):
    def test_complete_pair_passes(self) -> None:
        server = b"\n".join(MODULE.SERVER_MARKERS)
        client = b"\n".join(MODULE.CLIENT_MARKERS)
        result = MODULE.classify(server, client)
        self.assertTrue(result["pass"])
        self.assertEqual(result["failure_markers"], [])

    def test_missing_disconnect_fails(self) -> None:
        server = b"\n".join(MODULE.SERVER_MARKERS)
        client = b"\n".join(MODULE.CLIENT_MARKERS[:-1])
        result = MODULE.classify(server, client)
        self.assertFalse(result["pass"])
        self.assertEqual(
            result["client_missing"], ["RFDBG_SLE_S2_CONNECT_DISCONNECT_OK"]
        )

    def test_failure_marker_fails_closed(self) -> None:
        server = b"\n".join(MODULE.SERVER_MARKERS)
        client = b"\n".join((*MODULE.CLIENT_MARKERS, MODULE.FAILURE_MARKERS[0]))
        result = MODULE.classify(server, client)
        self.assertFalse(result["pass"])
        self.assertEqual(result["failure_markers"], ["RFDBG_MISSING_ROM_CALLBACK"])


if __name__ == "__main__":
    unittest.main()
