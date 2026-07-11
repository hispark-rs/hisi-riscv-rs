#!/usr/bin/env python3
"""Regression tests for the HiSilicon relocation patcher."""

import runpy
import unittest
from pathlib import Path


PATCHER = runpy.run_path(str(Path(__file__).with_name("rf-patch-reloc58-from-layout.py")))
encode_branchi = PATCHER["encode_branchi"]
VERIFIER = runpy.run_path(str(Path(__file__).with_name("rf-verify-oracle-layout.py")))
find_self_call_offsets = VERIFIER["find_self_call_offsets"]


class BranchiEncodingTests(unittest.TestCase):
    def test_forward_offsets_match_vendor_linker(self) -> None:
        base = 0x1105_003B
        self.assertEqual(encode_branchi(base, 0x004), 0x1105_013B)
        self.assertEqual(encode_branchi(base, 0x020), 0x1105_083B)
        self.assertEqual(encode_branchi(base, 0x086), 0x1125_01BB)
        self.assertEqual(encode_branchi(base, 0x094), 0x1125_053B)

    def test_backward_offset_matches_vendor_linker(self) -> None:
        base = 0x1705_103B
        self.assertEqual(encode_branchi(base, -0x16), 0x17F5_1ABB)

    def test_rejects_invalid_offsets(self) -> None:
        with self.assertRaises(ValueError):
            encode_branchi(0, 3)
        with self.assertRaises(ValueError):
            encode_branchi(0, 0x200)


class SelfCallPlaceholderTests(unittest.TestCase):
    def test_detects_unresolved_weak_call(self) -> None:
        placeholder = b"\x97\x00\x00\x00\xe7\x80\x00\x00"
        self.assertEqual(find_self_call_offsets(b"abc" + placeholder + b"xyz"), [3])

    def test_ignores_normal_call(self) -> None:
        self.assertEqual(
            find_self_call_offsets(b"\x97\x00\x00\x00\xe7\x80\x40\x00"), []
        )


if __name__ == "__main__":
    unittest.main()
