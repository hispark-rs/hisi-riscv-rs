#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyserial"]
# ///
"""Host tests for the unchanged-image WS63 connectivity reset matrix."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("ws63-connectivity-reset-matrix.py")
SPEC = importlib.util.spec_from_file_location("ws63_connectivity_reset_matrix", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MATRIX = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MATRIX)


def a5b_success_log() -> bytes:
    return b"\n".join(
        (
            b"W2E_AP_SECURITY mode=transition",
            b"RFDBG_A5B_INITIALIZE_OK elapsed_ms=0x00000024",
            b"RFDBG_A5B_SCAN_OK elapsed_ms=0x00000620 count=0x00000009 truncated=0x00000000",
            b"RFDBG_A5B_CONNECT_OK elapsed_ms=0x0000002a",
            b"RFDBG_A5B_DISCONNECT_OK elapsed_ms=0x00000009",
            (
                b"RFDBG_A5B_CONNECT_ASSOC_IOCTL "
                b"0x00000003 0x0000001c 0x00000020 "
                b"0x00000000 0x00000000 0x00000000 "
                b"0x00000000 0x00000000 0x00000000 "
                b"0x00000001 0x00000016 0x00000016"
            ),
            b"RFDBG_A5B_RUNNER_ELAPSED_MS value=0x0000000c",
            b"RFDBG_A5B_RUNNER_ELAPSED_MS value=0x00000026",
            b"RFDBG_A5B_EVENT pending=0x00000004 high_water=0x00000004 dropped=0x00000000",
            b"RFDBG_A5B_CONTROL pending=0x00000000 high_water=0x00000001",
            (
                b"RFDBG_A5B_RUNNER run=0x00000018 waits=0x00000018 wake=0x00000018 "
                b"immediate=0x00000008 operations=0x00000004 completed=0x00000004 "
                b"pending=0x0000000e exhausted=0x00000002 errors=0x00000000"
            ),
            (
                b"RFDBG_A5B_WAIT backend=0x00000033 l2=0x00000000 waker=0x00000033 "
                b"polls=0x00000019 pending=0x00000009 ready=0x00000010 timer=0x00000002"
            ),
            (
                b"RFDBG_A5B_BLOCKING init_calls=0x00000001 init_max_ms=0x0000003e "
                b"scan_calls=0x00000000 poll_calls=0x00000000 internal_sleep=0x00000000 "
                b"supplicant_poll=0x00000000"
            ),
            b"RFDBG_A5B_CONNECT_PROFILE_OK",
        )
    )


class ClassifyTests(unittest.TestCase):
    def test_pure_wpa3_gate_accepts_matching_success(self) -> None:
        log = b"\n".join(
            (
                b"W2E_AP_SECURITY mode=pure-wpa3",
                b"W2E_WPA3_CONNECT_OK pmf=required",
            )
        )
        self.assertEqual(MATRIX.classify(log, "rust", "connect", "pure-wpa3"), "pass")

    def test_pure_wpa3_gate_rejects_transition_success(self) -> None:
        log = b"\n".join(
            (
                b"W2E_AP_SECURITY mode=transition",
                b"W2E_WPA3_CONNECT_OK pmf=required",
            )
        )
        self.assertEqual(
            MATRIX.classify(log, "rust", "connect", "pure-wpa3"),
            "wrong_ap_mode",
        )

    def test_missing_mode_does_not_hide_connect_error(self) -> None:
        log = b"W2E_WPA3_CONNECT_ERR code=0x1"
        self.assertEqual(
            MATRIX.classify(log, "rust", "connect", "pure-wpa3"),
            "connect_error",
        )

    def test_pure_wpa3_gate_rejects_success_without_mode_evidence(self) -> None:
        log = b"W2E_WPA3_CONNECT_OK pmf=required"
        self.assertEqual(
            MATRIX.classify(log, "rust", "connect", "pure-wpa3"),
            "missing_ap_mode",
        )

    def test_transition_gate_accepts_matching_a5b_success(self) -> None:
        log = a5b_success_log()
        self.assertEqual(MATRIX.classify(log, "rust", "connect", "transition"), "pass")

    def test_transition_gate_rejects_a5b_success_without_mode_evidence(self) -> None:
        log = a5b_success_log().replace(b"W2E_AP_SECURITY mode=transition\n", b"")
        self.assertEqual(
            MATRIX.classify(log, "rust", "connect", "transition"),
            "missing_ap_mode",
        )

    def test_a5b_success_requires_complete_metric_trailer(self) -> None:
        log = b"\n".join(
            (
                b"W2E_AP_SECURITY mode=transition",
                b"RFDBG_A5B_CONNECT_PROFILE_OK",
            )
        )
        self.assertEqual(
            MATRIX.classify(log, "rust", "connect", "transition"),
            "missing_a5b_metrics",
        )

    def test_a5b_metric_parser_preserves_counts_and_maximum_step(self) -> None:
        metrics = MATRIX.parse_a5b_metrics(a5b_success_log())
        self.assertIsNotNone(metrics)
        assert metrics is not None
        self.assertTrue(metrics["complete"])
        self.assertEqual(metrics["missing"], [])
        self.assertEqual(metrics["event_queue"]["high_water"], 4)
        self.assertEqual(metrics["runner"]["exhausted"], 2)
        self.assertEqual(metrics["wait"]["waker"], 0x33)
        self.assertEqual(metrics["blocking"]["internal_sleep"], 0)
        self.assertEqual(metrics["timings"]["initialize_elapsed_ms"], 0x24)
        self.assertEqual(metrics["timings"]["scan_elapsed_ms"], 0x620)
        self.assertEqual(metrics["timings"]["connect_elapsed_ms"], 0x2A)
        self.assertEqual(metrics["timings"]["disconnect_elapsed_ms"], 9)
        self.assertEqual(metrics["timings"]["association_ioctl_max_ms"], 0x20)
        self.assertEqual(metrics["timings"]["runner_step_max_ms"], 0x26)
        self.assertEqual(metrics["timings"]["runner_step_count"], 2)

    def test_a5b_metric_aggregate_reports_range_and_missing_runs(self) -> None:
        complete = MATRIX.parse_a5b_metrics(a5b_success_log())
        incomplete = MATRIX.parse_a5b_metrics(
            a5b_success_log().replace(b"RFDBG_A5B_WAIT backend=", b"RFDBG_A5B_WAIT missing=")
        )
        aggregate = MATRIX.aggregate_a5b_metrics(
            [
                {"a5b_metrics": complete},
                {"a5b_metrics": incomplete},
                {"a5b_metrics": None},
            ]
        )
        self.assertEqual(aggregate["runs_with_markers"], 2)
        self.assertEqual(aggregate["complete_runs"], 1)
        self.assertEqual(aggregate["missing_runs"], 1)
        self.assertEqual(
            aggregate["ranges"]["timings.runner_step_max_ms"],
            {"min": 0x26, "max": 0x26},
        )

    def test_record_and_summary_share_live_and_offline_semantics(self) -> None:
        record = MATRIX.record_from_log(
            1,
            a5b_success_log(),
            "rust",
            "connect",
            "transition",
            "run-01.uart.log",
        )
        summary = MATRIX.summarize_records(
            [record],
            port=None,
            baud=115_200,
            profile="rust",
            stage="connect",
            required_ap_mode="transition",
            timeout=65.0,
            post_terminal_seconds=1.0,
            reference_ping=None,
            source_dir=Path("/evidence"),
        )
        self.assertEqual(summary["counts"], {"pass": 1})
        self.assertEqual(summary["a5b_metrics"]["complete_runs"], 1)
        self.assertEqual(summary["records"][0]["result"], "pass")

    def test_terminal_capture_can_finish_while_uart_is_silent(self) -> None:
        self.assertFalse(MATRIX.post_terminal_elapsed(None, 2.0, 100.0))
        self.assertFalse(MATRIX.post_terminal_elapsed(10.0, 2.0, 11.999))
        self.assertTrue(MATRIX.post_terminal_elapsed(10.0, 2.0, 12.0))


if __name__ == "__main__":
    unittest.main()
