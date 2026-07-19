#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("check-wpa-profile.py")
SPEC = importlib.util.spec_from_file_location("check_wpa_profile", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class CmakeSourceParsingTest(unittest.TestCase):
    def test_ignores_full_line_and_trailing_comments(self):
        block = """
            ${ROOT_DIR}/kept.c
            # ${ROOT_DIR}/disabled.c
            ${ROOT_DIR}/also-kept.c # ${ROOT_DIR}/not-this.c
        """
        self.assertEqual(MODULE.cmake_sources(block), ["/also-kept.c", "/kept.c"])

    def test_deduplicates_sources(self):
        block = "${ROOT_DIR}/same.c\n${ROOT_DIR}/same.c"
        self.assertEqual(MODULE.cmake_sources(block), ["/same.c"])


if __name__ == "__main__":
    unittest.main()
