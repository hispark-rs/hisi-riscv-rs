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
            b"RF5A_ARP_OK evidence=l2-arp-reply",
            (
                b"RF5C_LOCAL_DATA_PATH_OK arp_reply=0x00000001 "
                b"arp_request=0x00000001 gateway=192.0.2.1"
            ),
            b"RF5C_PUBLIC_DNS_BEGIN target=223.5.5.5 attempts=0x00000003",
            (
                b"RF5C_PUBLIC_DNS_SAMPLE attempt=0x00000001 txid=0x00005754 "
                b"status=ok answers=0x00000001"
            ),
            (
                b"RF5C_PUBLIC_DNS_OK target=223.5.5.5 attempts=0x00000001 "
                b"responses=0x00000001 invalid=0x00000000 tx_error=0x00000000"
            ),
            (
                b"RF5C_CONNECTIVITY_SUMMARY arp_request=0x00000001 "
                b"arp_reply=0x00000001 dns_attempts=0x00000001 "
                b"dns_responses=0x00000001 dns_invalid=0x00000000 "
                b"dns_tx_error=0x00000000 rx_queue_drop=0x00000000"
            ),
            (
                b"RFDBG_A5B_L2 rx_arp_req=0x00000001 rx_arp_reply=0x00000001 "
                b"rx_ipv4=0x00000004 rx_other=0x00000000 tx_arp_req=0x00000001 "
                b"tx_arp_reply=0x00000000 tx_ipv4=0x0000000c tx_other=0x00000000"
            ),
            b"A4_NET_RUNNER_STEADY lease=managed neighbor_cache=managed",
            b"A4_DHCP_RENEW_OK client=0x00000001 server=0x00000001",
        )
    )


class ClassifyTests(unittest.TestCase):
    def test_connect_error_is_not_masked_by_incomplete_a5b_metrics(self) -> None:
        log = b"\n".join(
            (
                b"RF5B_CONNECT_ERR: code=backend.other stage=operation backend=0x5732b003",
                b"RFDBG_A5B_CONNECT_PROFILE_OK",
            )
        )
        self.assertEqual(
            MATRIX.classify(log, "rust", "connectivity"),
            "connect_error",
        )

    def test_connectivity_record_keeps_l2_protocol_diagnostics(self) -> None:
        record = MATRIX.record_from_log(
            1,
            connectivity_success_log(),
            "rust",
            "connectivity",
            None,
            "run-01.uart.log",
        )
        self.assertEqual(
            record["l2_protocol"],
            {
                "rx_arp_req": 1,
                "rx_arp_reply": 1,
                "rx_ipv4": 4,
                "rx_other": 0,
                "tx_arp_req": 1,
                "tx_arp_reply": 0,
                "tx_ipv4": 12,
                "tx_other": 0,
            },
        )

    def test_l2_protocol_aggregate_reports_ranges_and_missing_runs(self) -> None:
        record = MATRIX.record_from_log(
            1,
            connectivity_success_log(),
            "rust",
            "connectivity",
            None,
            "run-01.uart.log",
        )
        aggregate = MATRIX.aggregate_l2_protocol_diagnostics(
            [record, {"l2_protocol": None}]
        )
        self.assertEqual(aggregate["runs_with_markers"], 1)
        self.assertEqual(aggregate["runs_without_markers"], 1)
        self.assertEqual(
            aggregate["ranges"]["rx_arp_reply"],
            {"min": 1, "max": 1},
        )
        self.assertEqual(
            aggregate["ranges"]["tx_ipv4"],
            {"min": 12, "max": 12},
        )

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

    def test_connectivity_record_and_summary_use_stage_complete_metrics(self) -> None:
        trailer = a5b_success_log().replace(
            b"RFDBG_A5B_DISCONNECT_OK elapsed_ms=0x00000009\n",
            b"",
        )
        record = MATRIX.record_from_log(
            1,
            b"\n".join((connectivity_success_log(), trailer)),
            "rust",
            "connectivity",
            None,
            "run-01.uart.log",
            require_contract=True,
            max_runner_step_ms=100,
        )
        summary = MATRIX.summarize_records(
            [record],
            port=None,
            baud=115_200,
            profile="rust",
            stage="connectivity",
            required_ap_mode=None,
            timeout=90.0,
            post_terminal_seconds=1.0,
            reference_ping=None,
        )
        self.assertEqual(record["result"], "pass")
        self.assertEqual(record["dns"]["status"], "ok")
        self.assertEqual(summary["dns_observations"], {"ok": 1})
        self.assertEqual(summary["dns_totals"]["responses"], 1)
        self.assertTrue(record["a5b_metrics"]["complete"])
        self.assertEqual(record["a5b_metrics"]["missing"], [])
        self.assertEqual(summary["a5b_metrics"]["complete_runs"], 1)
        self.assertEqual(summary["a5b_metrics"]["missing_runs"], 0)

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

    def test_valid_public_dns_response_passes_connectivity(self) -> None:
        log = connectivity_success_log()
        self.assertEqual(
            MATRIX.classify(
                log,
                "rust",
                "connectivity",
                require_contract=True,
            ),
            "pass",
        )

    def test_gateway_loss_is_a_local_data_path_failure(self) -> None:
        log = (
            connectivity_success_log()
            .replace(
                b"RF5C_LOCAL_DATA_PATH_OK arp_reply=0x00000001",
                b"RF5C_LOCAL_DATA_PATH_ERR arp_reply=0x00000000",
            )
            .replace(b"arp_reply=0x00000001", b"arp_reply=0x00000000")
        )
        self.assertEqual(
            MATRIX.classify(log, "rust", "connectivity"),
            "local_data_path_failure",
        )
        self.assertIn(
            "missing:local_data_path",
            MATRIX.validate_rust_contract(log, "connectivity"),
        )

    def test_dns_failure_is_not_masked_by_local_connectivity(self) -> None:
        log = connectivity_success_log().replace(
            b"RF5C_PUBLIC_DNS_OK target=",
            b"RF5C_PUBLIC_DNS_ERR target=",
        )
        self.assertEqual(
            MATRIX.classify(log, "rust", "connectivity"),
            "public_dns_failure",
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

    def test_connectivity_contract_rejects_dns_tx_error(self) -> None:
        log = connectivity_success_log().replace(
            b"dns_tx_error=0x00000000",
            b"dns_tx_error=0x00000001",
        ).replace(
            b"tx_error=0x00000000",
            b"tx_error=0x00000001",
        )
        violations = MATRIX.validate_rust_contract(log, "connectivity")
        self.assertIn("nonzero:summary.dns_tx_error", violations)
        self.assertIn("nonzero:public_dns.tx_error", violations)

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

    def test_connectivity_a5b_trailer_does_not_require_disconnect(self) -> None:
        trailer = a5b_success_log().replace(
            b"RFDBG_A5B_DISCONNECT_OK elapsed_ms=0x00000009\n",
            b"",
        )
        log = b"\n".join((connectivity_success_log(), trailer))
        metrics = MATRIX.parse_a5b_metrics(
            trailer,
            require_disconnect=False,
        )
        self.assertIsNotNone(metrics)
        assert metrics is not None
        self.assertTrue(metrics["complete"])
        self.assertNotIn("disconnect_elapsed_ms", metrics["timings"])
        self.assertEqual(
            MATRIX.validate_rust_contract(
                log,
                "connectivity",
                max_runner_step_ms=100,
            ),
            [],
        )
        self.assertIn(
            "a5b_metrics_incomplete:disconnect_elapsed_ms",
            MATRIX.validate_rust_contract(
                trailer,
                "connect",
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
