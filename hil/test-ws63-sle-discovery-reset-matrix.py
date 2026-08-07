#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Unit tests for the SLE paired-board marker contract."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("ws63-sle-discovery-reset-matrix.py")
SPEC = importlib.util.spec_from_file_location("ws63_sle_matrix", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ClassifyTests(unittest.TestCase):
    def test_complete_pair_passes(self) -> None:
        announce = b"\n".join(MODULE.ANNOUNCE_MARKERS)
        seek = b"\n".join(MODULE.SEEK_MARKERS)
        result = MODULE.classify(announce, seek)
        self.assertTrue(result["pass"])
        self.assertEqual(result["failure_markers"], [])

    def test_missing_seek_match_fails(self) -> None:
        announce = b"\n".join(MODULE.ANNOUNCE_MARKERS)
        seek = b"\n".join(MODULE.SEEK_MARKERS[:-1])
        result = MODULE.classify(announce, seek)
        self.assertFalse(result["pass"])
        self.assertEqual(result["seek_missing"], ["RFDBG_SLE_S1_SEEK_MATCH"])

    def test_failure_marker_fails_closed(self) -> None:
        announce = b"\n".join(MODULE.ANNOUNCE_MARKERS)
        seek = b"\n".join((*MODULE.SEEK_MARKERS, MODULE.FAILURE_MARKERS[0]))
        result = MODULE.classify(announce, seek)
        self.assertFalse(result["pass"])
        self.assertEqual(result["failure_markers"], ["RFDBG_MISSING_ROM_CALLBACK"])


if __name__ == "__main__":
    unittest.main()
