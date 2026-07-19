#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.9"
# dependencies = ["tomli>=2.0.1; python_version < '3.11'"]
# ///
"""Regression tests for the WS63 supplicant final-link boundary."""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("check-ws63-supplicant-boundary.py")
SPEC = importlib.util.spec_from_file_location("ws63_supplicant_boundary", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
BOUNDARY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BOUNDARY)


class SupplicantBoundaryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.profile = {
            "native_archives": [
                {
                    "profile": "personal",
                    "archive": "libhisi_wpa_native_port.a",
                },
                {
                    "profile": "personal-wpa3",
                    "archive": "libhisi_wpa_native_port_wpa3.a",
                },
            ],
            "native_object_markers": ["hisi_wpa_port.o", "supplicant_ws63.o"],
            "legacy_archives": ["libwpa_supplicant.a", "libmbedtls_v3.6.0.a"],
            "legacy_provider_symbols": ["wifi_is_need_psk", "LOS_TaskResRecycle"],
        }

    def test_accepts_native_archive_without_legacy_inputs(self) -> None:
        BOUNDARY.check_map(
            self.profile,
            "/tmp/out/libws63_radio_sys.rlib(hash-hisi_wpa_port.o)\n"
            "/tmp/out/libws63_radio_sys.rlib(hash-supplicant_ws63.o)\n",
        )
        BOUNDARY.check_symbols(self.profile, {"hisi_wpa_create", "main"})

    def test_rejects_legacy_archive(self) -> None:
        with self.assertRaisesRegex(BOUNDARY.BoundaryError, "legacy archives"):
            BOUNDARY.check_map(
                self.profile,
                "/tmp/out/libws63_radio_sys.rlib(hash-hisi_wpa_port.o)\n"
                "/tmp/out/libws63_radio_sys.rlib(hash-supplicant_ws63.o)\n"
                "/tmp/vendor/libwpa_supplicant.a(events.o)\n",
            )

    def test_rejects_missing_native_archive(self) -> None:
        with self.assertRaisesRegex(BOUNDARY.BoundaryError, "native supplicant"):
            BOUNDARY.check_map(
                self.profile,
                "/tmp/out/libws63_radio_sys.rlib(hash-hisi_wpa_port.o)\n",
            )

    def test_rejects_legacy_provider_symbol(self) -> None:
        with self.assertRaisesRegex(BOUNDARY.BoundaryError, "provider symbols"):
            BOUNDARY.check_symbols(self.profile, {"main", "wifi_is_need_psk"})


if __name__ == "__main__":
    unittest.main()
