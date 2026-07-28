#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyserial"]
# ///
"""Host tests for the unchanged-image WS63 connectivity reset matrix."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
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


def connectivity_success_log() -> bytes:
    return b"\n".join(
        (
            b"RF1_IMAGE_OK",
            b"RF2_INIT_OK ifname=hisi-rf",
            b"A4_RADIO_EVENT kind=initialized",
            b"RF3_SCAN_OK count=0x00000003",
            b"A4_RADIO_EVENT kind=scan-completed",
            b"W2D_WPA2_CONNECT_OK",
            b"A4_RADIO_EVENT kind=connected",
            b"RF5A_DHCP_OK addr=192.0.2.2",
            b"RF5A_ARP_OK mode=smoltcp-neighbor-cache",
            (
                b"RF5C_PING_OK target=1.1.1.1 tx=0x00000005 rx=0x00000004 "
                b"drop=0x00000001 tx_error=0x00000000 rx_queue_drop=0x00000000"
            ),
            (
                b"RF5C_CONNECTIVITY_SUMMARY gateway_tx=0x00000005 "
                b"gateway_rx=0x00000000 public_tx=0x00000005 "
                b"public_rx=0x00000004 rx_queue_drop=0x00000000"
            ),
            b"A4_NET_RUNNER_STEADY lease=managed neighbor_cache=managed",
            b"A4_DHCP_RENEW_OK client=0x00000001 server=0x00000001",
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

    def test_connectivity_contract_accepts_ordered_zero_error_log(self) -> None:
        log = connectivity_success_log()
        self.assertEqual(MATRIX.validate_rust_contract(log, "connectivity"), [])
        self.assertEqual(
            MATRIX.classify(
                log,
                "rust",
                "connectivity",
                require_contract=True,
            ),
            "pass",
        )

    def test_connectivity_contract_fails_closed_on_missing_renew(self) -> None:
        log = connectivity_success_log().replace(
            b"A4_DHCP_RENEW_OK client=0x00000001 server=0x00000001",
            b"",
        )
        self.assertIn(
            "missing:dhcp_renew",
            MATRIX.validate_rust_contract(log, "connectivity"),
        )
        self.assertEqual(
            MATRIX.classify(
                log,
                "rust",
                "connectivity",
                require_contract=True,
            ),
            "contract_violation",
        )

    def test_connectivity_contract_rejects_nonzero_queue_drop(self) -> None:
        log = connectivity_success_log().replace(
            b"rx_queue_drop=0x00000000",
            b"rx_queue_drop=0x00000001",
        )
        violations = MATRIX.validate_rust_contract(log, "connectivity")
        self.assertIn("nonzero:summary.rx_queue_drop", violations)
        self.assertIn("nonzero:public_ping.rx_queue_drop", violations)

    def test_qemu_fixture_is_contract_only_and_rejected_as_silicon(self) -> None:
        log = b"\n".join(
            (
                MATRIX.QEMU_CONTRACT_FIXTURE_MARKER,
                connectivity_success_log(),
            )
        )
        self.assertEqual(
            MATRIX.validate_rust_contract(
                log,
                "connectivity",
                qemu_contract_fixture=True,
            ),
            [],
        )
        self.assertIn(
            "forbidden:qemu_contract_fixture",
            MATRIX.validate_rust_contract(log, "connectivity"),
        )

    def test_qemu_fixture_mode_requires_explicit_marker(self) -> None:
        self.assertIn(
            "missing:qemu_contract_fixture",
            MATRIX.validate_rust_contract(
                connectivity_success_log(),
                "connectivity",
                qemu_contract_fixture=True,
            ),
        )

    def test_a5b_contract_rejects_budget_and_backend_errors(self) -> None:
        log = (
            a5b_success_log()
            .replace(b"errors=0x00000000", b"errors=0x00000001")
            .replace(
                b"RFDBG_A5B_RUNNER_ELAPSED_MS value=0x00000026",
                b"RFDBG_A5B_RUNNER_ELAPSED_MS value=0x00000065",
            )
        )
        violations = MATRIX.validate_rust_contract(
            log, "connect", max_runner_step_ms=100
        )
        self.assertIn("nonzero:runner.errors", violations)
        self.assertIn("budget:runner_step_max_ms=101>100", violations)

    def test_declared_response_bound_requires_a5b_trailer(self) -> None:
        self.assertIn(
            "missing:a5b_connect_profile",
            MATRIX.validate_rust_contract(
                connectivity_success_log(),
                "connectivity",
                max_runner_step_ms=100,
            ),
        )

    def test_connectivity_stage_enforces_a5b_response_bound(self) -> None:
        log = b"\n".join((connectivity_success_log(), a5b_success_log()))
        self.assertEqual(
            MATRIX.validate_rust_contract(
                log,
                "connectivity",
                max_runner_step_ms=100,
            ),
            [],
        )
        over_budget = log.replace(
            b"RFDBG_A5B_RUNNER_ELAPSED_MS value=0x00000026",
            b"RFDBG_A5B_RUNNER_ELAPSED_MS value=0x00000065",
        )
        self.assertIn(
            "budget:runner_step_max_ms=101>100",
            MATRIX.validate_rust_contract(
                over_budget,
                "connectivity",
                max_runner_step_ms=100,
            ),
        )

    def test_artifact_identity_detects_elf_and_profile_drift(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            elf = root / "firmware.elf"
            identity_path = root / "identity.json"
            elf.write_bytes(b"first")
            written = MATRIX.write_artifact_identity(
                identity_path, elf, "upstream-wpa2"
            )
            verified = MATRIX.verify_artifact_identity(
                identity_path, elf, "upstream-wpa2"
            )
            self.assertEqual(verified["elf_sha256"], written["elf_sha256"])
            self.assertEqual(
                json.loads(identity_path.read_text())["marker_contract"],
                MATRIX.MARKER_CONTRACT,
            )

            with self.assertRaisesRegex(ValueError, "profile_id"):
                MATRIX.verify_artifact_identity(
                    identity_path, elf, "upstream-wpa3"
                )
            elf.write_bytes(b"second")
            with self.assertRaisesRegex(ValueError, "elf_sha256"):
                MATRIX.verify_artifact_identity(
                    identity_path, elf, "upstream-wpa2"
                )

    def test_terminal_capture_can_finish_while_uart_is_silent(self) -> None:
        self.assertFalse(MATRIX.post_terminal_elapsed(None, 2.0, 100.0))
        self.assertFalse(MATRIX.post_terminal_elapsed(10.0, 2.0, 11.999))
        self.assertTrue(MATRIX.post_terminal_elapsed(10.0, 2.0, 12.0))


if __name__ == "__main__":
    unittest.main()
