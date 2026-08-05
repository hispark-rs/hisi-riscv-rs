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
            (
                b"RFDBG_A5B_SCHED ready_owner_err=0x00000000 "
                b"ready_dup=0x00000000 ready_wrong_bucket=0x00000000 "
                b"ready_bad_link=0x00000000"
            ),
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
            (
                b"RFDBG_A5B_SCHED ready_owner_err=0x00000000 "
                b"ready_dup=0x00000000 ready_wrong_bucket=0x00000000 "
                b"ready_bad_link=0x00000000"
            ),
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
            (
                b"RF5C_PUBLIC_DNS_BEGIN primary=223.5.5.5 "
                b"secondary=180.76.76.76 attempts=0x00000004"
            ),
            (
                b"RF5C_PUBLIC_DNS_SAMPLE attempt=0x00000001 txid=0x00005754 "
                b"target=223.5.5.5 status=ok answers=0x00000001"
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


def resource_calibration_log() -> bytes:
    return b"\n".join(
        (
            (
                b"RFDBG_RESOURCE schema=hisi-rf-resource-report/v8 "
                b"revision=ws63-wifi-test runtime_arena=0x0002e200 "
                b"rf_arena=0x0001be00 calibrated=0x00000000"
            ),
            (
                b"RFDBG_HEAP rtos_arena=0x0002e200 rtos_used=0x0002a1c4 "
                b"rtos_free=0x0000403c rtos_peak=0x0002b000 "
                b"rtos_allocs=0x0000001b rtos_failures=0x00000000 "
                b"rf_arena=0x0001be00 rf_used=0x00001668 "
                b"rf_free=0x0001a798 rf_peak=0x00002538 "
                b"rf_failures=0x00000000"
            ),
        )
    )


class ClassifyTests(unittest.TestCase):
    def test_peer_serial_is_polled_when_driver_reports_no_waiting_bytes(self) -> None:
        class DelayedSerial:
            in_waiting = 0

            def __init__(self) -> None:
                self.requested = 0

            def read(self, size: int) -> bytes:
                self.requested = size
                return b"RFDBG_SOFTAP_READY\n"

        port = DelayedSerial()
        self.assertEqual(
            MATRIX.drain_serial(port),
            b"RFDBG_SOFTAP_READY\n",
        )
        self.assertEqual(port.requested, 4096)

    def test_peer_serial_drains_only_the_reported_queue_when_ready(self) -> None:
        class ReadySerial:
            in_waiting = 7

            def __init__(self) -> None:
                self.requested = 0

            def read(self, size: int) -> bytes:
                self.requested = size
                return b"1234567"

        port = ReadySerial()
        self.assertEqual(MATRIX.drain_serial(port), b"1234567")
        self.assertEqual(port.requested, 7)

    def test_jlink_argv_selects_exact_probe_in_multi_rig_setup(self) -> None:
        self.assertEqual(
            MATRIX.jlink_argv(
                "JLinkExe", Path("reset.jlink"), "12345678"
            ),
            [
                "JLinkExe",
                "-NoGui",
                "1",
                "-SelectEmuBySN",
                "12345678",
                "-CommandFile",
                "reset.jlink",
            ],
        )

    def test_jlink_argv_rejects_non_decimal_serial(self) -> None:
        with self.assertRaisesRegex(ValueError, "decimal digits"):
            MATRIX.jlink_argv("JLinkExe", Path("reset.jlink"), "--help")

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

    def test_local_echo_path_preserves_samples_and_step_deltas(self) -> None:
        log = b"\n".join(
            (
                b"RFDBG_LOCAL_ECHO_PATH sequence=0x00000000 phase=send mac_ok=0x00000010 "
                b"dmac_rx=0x00000020 vendor_rx=0x00000003 rust_rx=0x00000003",
                b"RFDBG_LOCAL_ECHO_PATH sequence=0x00000001 phase=send mac_ok=0x00000012 "
                b"dmac_rx=0x00000024 vendor_rx=0x00000004 rust_rx=0x00000004",
                b"RFDBG_LOCAL_ECHO_PATH sequence=0x00000002 phase=send mac_ok=0x00000015 "
                b"dmac_rx=0x00000029 vendor_rx=0x00000004 rust_rx=0x00000004",
            )
        )
        record = MATRIX.record_from_log(
            1, log, "rust", "connectivity", None, "run-01.uart.log"
        )
        self.assertEqual(len(record["local_echo_path"]), 3)
        aggregate = MATRIX.aggregate_local_echo_path(
            [record, {"local_echo_path": None}]
        )
        self.assertEqual(aggregate["runs_with_markers"], 1)
        self.assertEqual(aggregate["runs_without_markers"], 1)
        self.assertEqual(aggregate["samples"], {"min": 3, "max": 3})
        self.assertEqual(
            aggregate["step_delta_ranges"]["mac_ok"],
            {"min": 2, "max": 3},
        )
        self.assertEqual(
            aggregate["step_delta_ranges"]["vendor_rx"],
            {"min": 0, "max": 1},
        )

    def test_dual_uart_echo_correlation_finds_submitted_reply_loss(self) -> None:
        sta_log = b"\n".join(
            (
                b"RFDBG_LOCAL_ECHO_PATH sequence=0x00000000 phase=send mac_ok=0x1",
                b"RFDBG_LOCAL_ECHO_PATH sequence=0x00000000 phase=receive mac_ok=0x2",
                b"RFDBG_LOCAL_ECHO_PATH sequence=0x00000001 phase=send mac_ok=0x2",
            )
        )
        ap_log = b"\n".join(
            (
                b"RFDBG_SOFTAP_READY",
                b"RFDBG_SOFTAP_ECHO_PATH sequence=0x00000000 vendor_tx_delta=0x1 "
                b"tx_complete_delta=0x1 tx_status_delta=0x1,0x0,",
                b"RFDBG_SOFTAP_ECHO_PATH sequence=0x00000001 vendor_tx_delta=0x1 "
                b"tx_complete_delta=0x1 tx_status_delta=0x1,0x0,",
            )
        )
        record = MATRIX.record_from_log(
            1,
            sta_log,
            "rust",
            "connectivity",
            None,
            "run-01.uart.log",
            peer_log=ap_log,
            peer_log_name="run-01.peer.uart.log",
        )
        self.assertTrue(record["peer_ready"])
        self.assertEqual(record["peer_echo_path"][0]["tx_status_delta"], [1, 0])
        self.assertEqual(
            record["echo_correlation"]["submitted_without_sta_receive"],
            [1],
        )

    def test_dual_uart_echo_correlation_aggregate_splits_results(self) -> None:
        passing = {
            "result": "pass",
            "echo_correlation": {
                "sta_sent": [0, 1],
                "sta_received": [0],
                "ap_observed": [0, 1],
                "ap_submitted": [0, 1],
                "sent_missing_at_ap": [],
                "submitted_without_sta_receive": [1],
            },
        }
        failing = {
            "result": "local_data_path_failure",
            "echo_correlation": {
                "sta_sent": [0, 1],
                "sta_received": [],
                "ap_observed": [0],
                "ap_submitted": [0],
                "sent_missing_at_ap": [1],
                "submitted_without_sta_receive": [0],
            },
        }
        aggregate = MATRIX.aggregate_echo_correlation(
            [passing, failing, {"result": "pass", "echo_correlation": None}]
        )
        self.assertEqual(aggregate["runs_with_markers"], 2)
        self.assertEqual(aggregate["runs_without_markers"], 1)
        self.assertEqual(aggregate["totals"]["sta_sent"], 4)
        self.assertEqual(aggregate["totals"]["sta_received"], 1)
        self.assertEqual(aggregate["by_result"]["pass"]["runs"], 1)
        self.assertEqual(
            aggregate["by_result"]["local_data_path_failure"]
            ["sent_missing_at_ap"],
            1,
        )

    def test_required_peer_capture_fails_closed_without_softap_ready(self) -> None:
        record = MATRIX.record_from_log(
            1,
            connectivity_success_log() + b"\n" + resource_calibration_log(),
            "rust",
            "connectivity",
            None,
            "run-01.uart.log",
            require_contract=True,
            require_resource_calibration=True,
            peer_log=b"",
            peer_log_name="run-01.peer.uart.log",
        )
        self.assertEqual(record["result"], "contract_violation")
        self.assertIn(
            "missing peer RFDBG_SOFTAP_READY marker",
            record["contract"]["violations"],
        )

    def test_irq45_lifecycle_is_preserved_for_both_boards(self) -> None:
        sta_log = (
            b"RFDBG_A5B_DATA_PATH tx=0x1 irq45=0x20 "
            b"irq45_en_calls=0x1 irq45_dis_calls=0x0 irq45_clr_calls=0x1f "
            b"irq45_enabled=0x1 irq45_pending=0x0"
        )
        ap_log = (
            b"RFDBG_SOFTAP_STATE event=0x1 irq45=0x30 "
            b"irq45_en_calls=0x1 irq45_dis_calls=0x1 irq45_clr_calls=0x2f "
            b"irq45_enabled=0x0 irq45_pending=0x1"
        )
        record = MATRIX.record_from_log(
            1,
            sta_log,
            "rust",
            "connectivity",
            None,
            "run-01.uart.log",
            peer_log=ap_log,
            peer_log_name="run-01.peer.uart.log",
        )
        self.assertEqual(record["irq45_lifecycle"]["irq45"], 0x20)
        self.assertEqual(record["peer_irq45_lifecycle"]["irq45_pending"], 1)

        summary = MATRIX.summarize_records(
            [record],
            port=None,
            baud=115_200,
            profile="rust",
            stage="connectivity",
            required_ap_mode=None,
            timeout=1,
            post_terminal_seconds=0,
            reference_ping=None,
        )
        self.assertEqual(summary["irq45_lifecycle"]["disabled_runs"], 0)
        self.assertEqual(summary["peer_irq45_lifecycle"]["disabled_runs"], 1)
        self.assertEqual(summary["peer_irq45_lifecycle"]["pending_runs"], 1)

    def test_irq45_lifecycle_requires_all_fields(self) -> None:
        self.assertIsNone(
            MATRIX.parse_irq45_lifecycle(
                b"RFDBG_A5B_DATA_PATH irq45=0x20 irq45_enabled=0x1",
                b"RFDBG_A5B_DATA_PATH ",
            )
        )

    def test_offline_discovery_does_not_count_peer_logs_as_runs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "run-01.uart.log").write_bytes(b"sta")
            (root / "run-01.peer.uart.log").write_bytes(b"ap")
            (root / "run-02.uart.log").write_bytes(b"sta")
            self.assertEqual(
                [path.name for path in MATRIX.measured_uart_logs(root)],
                ["run-01.uart.log", "run-02.uart.log"],
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
            MATRIX.parse_ready_ownership(log),
            {
                "ready_owner_err": 0,
                "ready_dup": 0,
                "ready_wrong_bucket": 0,
                "ready_bad_link": 0,
            },
        )
        self.assertEqual(
            MATRIX.classify(
                log,
                "rust",
                "connectivity",
                require_contract=True,
            ),
            "pass",
        )

    def test_connectivity_contract_rejects_ready_ownership_violation(self) -> None:
        log = connectivity_success_log().replace(
            b"ready_owner_err=0x00000000",
            b"ready_owner_err=0x00000001",
        )
        self.assertIn(
            "nonzero:sta.ready_ownership.ready_owner_err=1",
            MATRIX.validate_rust_contract(log, "connectivity"),
        )

    def test_peer_contract_rejects_ready_queue_link_violation(self) -> None:
        peer_log = (
            b"RFDBG_SOFTAP_READY\n"
            b"RFDBG_SOFTAP_SCHED ready_owner_err=0x00000000 "
            b"ready_dup=0x00000000 ready_wrong_bucket=0x00000000 "
            b"ready_bad_link=0x00000001\n"
        )
        record = MATRIX.record_from_log(
            1,
            connectivity_success_log(),
            "rust",
            "connectivity",
            None,
            "run-01.uart.log",
            require_contract=True,
            peer_log=peer_log,
            peer_log_name="run-01.peer.uart.log",
        )
        self.assertEqual(record["result"], "contract_violation")
        self.assertIn(
            "nonzero:peer.ready_ownership.ready_bad_link=1",
            record["contract"]["violations"],
        )

    def test_ready_ownership_hil_marker_requires_paired_zero_contract(self) -> None:
        peer_log = (
            b"RFDBG_SOFTAP_READY\n"
            b"RFDBG_SOFTAP_SCHED ready_owner_err=0x00000000 "
            b"ready_dup=0x00000000 ready_wrong_bucket=0x00000000 "
            b"ready_bad_link=0x00000000\n"
        )
        record = MATRIX.record_from_log(
            1,
            connectivity_success_log(),
            "rust",
            "connectivity",
            None,
            "run-01.uart.log",
            require_contract=True,
            peer_log=peer_log,
            peer_log_name="run-01.peer.uart.log",
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
        self.assertEqual(summary["evidence_markers"], ["A5R_READY_OWNERSHIP_OK"])

        record["peer_ready_ownership"]["ready_dup"] = 1
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
        self.assertEqual(summary["evidence_markers"], [])

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

    def test_isolated_softap_accepts_local_neighbor_without_public_route(self) -> None:
        log = connectivity_success_log()
        for line in (
            (
                b"RF5C_PUBLIC_DNS_BEGIN primary=223.5.5.5 "
                b"secondary=180.76.76.76 attempts=0x00000004"
            ),
            (
                b"RF5C_PUBLIC_DNS_SAMPLE attempt=0x00000001 txid=0x00005754 "
                b"target=223.5.5.5 status=ok answers=0x00000001"
            ),
            (
                b"RF5C_PUBLIC_DNS_OK target=223.5.5.5 attempts=0x00000001 "
                b"responses=0x00000001 invalid=0x00000000 tx_error=0x00000000"
            ),
        ):
            log = log.replace(line, b"")
        log = log.replace(
            b"RF5A_ARP_OK evidence=l2-arp-reply",
            b"RF5C_PUBLIC_DNS_SKIP reason=no-default-route\n"
            b"RF5A_ARP_OK evidence=l2-arp-reply",
        )
        log = log.replace(
            b"RF5C_CONNECTIVITY_SUMMARY arp_request=0x00000001 "
            b"arp_reply=0x00000001 dns_attempts=0x00000001 "
            b"dns_responses=0x00000001 dns_invalid=0x00000000 "
            b"dns_tx_error=0x00000000 rx_queue_drop=0x00000000",
            b"RF5C_CONNECTIVITY_SUMMARY arp_request=0x00000001 "
            b"arp_reply=0x00000001 dns_attempts=0x00000000 "
            b"dns_responses=0x00000000 dns_invalid=0x00000000 "
            b"dns_tx_error=0x00000000 rx_queue_drop=0x00000000",
        )

        self.assertEqual(MATRIX.validate_rust_contract(log, "connectivity"), [])
        self.assertEqual(
            MATRIX.classify(log, "rust", "connectivity", require_contract=True),
            "pass",
        )

    def test_secondary_public_dns_response_passes_connectivity(self) -> None:
        log = connectivity_success_log().replace(
            b"RF5C_PUBLIC_DNS_OK target=223.5.5.5",
            b"RF5C_PUBLIC_DNS_OK target=180.76.76.76",
        )
        self.assertEqual(MATRIX.validate_rust_contract(log, "connectivity"), [])

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

    def test_resource_calibration_accepts_matching_zero_failure_metrics(self) -> None:
        calibration = MATRIX.parse_resource_calibration(resource_calibration_log())
        self.assertIsNotNone(calibration)
        assert calibration is not None
        self.assertTrue(calibration["complete"])
        self.assertEqual(calibration["violations"], [])
        self.assertEqual(calibration["heap"]["rtos_peak"], 0x2B000)

    def test_resource_calibration_fails_closed_on_missing_marker(self) -> None:
        violations = MATRIX.validate_rust_contract(
            connectivity_success_log(),
            "connectivity",
            require_resource_calibration=True,
        )
        self.assertIn("missing:resource_calibration", violations)

    def test_resource_calibration_rejects_failures_and_arena_drift(self) -> None:
        log = (
            resource_calibration_log()
            .replace(b"rtos_failures=0x00000000", b"rtos_failures=0x00000001")
            .replace(b"rf_arena=0x0001be00 rf_used", b"rf_arena=0x0001bd00 rf_used")
            .replace(b"rf_free=0x0001a798", b"rf_free=0x0001a698")
        )
        calibration = MATRIX.parse_resource_calibration(log)
        self.assertIsNotNone(calibration)
        assert calibration is not None
        self.assertIn("nonzero:rtos.failures", calibration["violations"])
        self.assertIn("mismatch:rf.arena", calibration["violations"])

    def test_resource_calibration_summary_reports_peak_headroom(self) -> None:
        record = MATRIX.record_from_log(
            1,
            b"\n".join(
                (
                    connectivity_success_log(),
                    a5b_success_log().replace(
                        b"RFDBG_A5B_DISCONNECT_OK elapsed_ms=0x00000009\n",
                        b"",
                    ),
                    resource_calibration_log(),
                )
            ),
            "rust",
            "connectivity",
            None,
            "run-01.uart.log",
            require_contract=True,
            max_runner_step_ms=100,
            require_resource_calibration=True,
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
        calibration = summary["resource_calibration"]
        self.assertEqual(calibration["complete_runs"], 1)
        self.assertEqual(calibration["runtime_headroom_remaining_bytes"], 0x3200)
        self.assertEqual(calibration["rf_headroom_remaining_bytes"], 0x198C8)

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

    def test_optional_peer_identity_is_all_or_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            elf = root / "peer.elf"
            identity_path = root / "peer.identity.json"
            elf.write_bytes(b"peer")
            MATRIX.write_artifact_identity(identity_path, elf, "softap-wpa3")
            self.assertIsNone(
                MATRIX.verify_optional_identity(None, None, None, label="peer")
            )
            with self.assertRaisesRegex(ValueError, "peer identity requires"):
                MATRIX.verify_optional_identity(
                    identity_path, elf, None, label="peer"
                )
            verified = MATRIX.verify_optional_identity(
                identity_path, elf, "softap-wpa3", label="peer"
            )
            self.assertIsNotNone(verified)
            assert verified is not None
            self.assertEqual(verified["profile_id"], "softap-wpa3")

    def test_terminal_capture_can_finish_while_uart_is_silent(self) -> None:
        self.assertFalse(MATRIX.post_terminal_elapsed(None, 2.0, 100.0))
        self.assertFalse(MATRIX.post_terminal_elapsed(10.0, 2.0, 11.999))
        self.assertTrue(MATRIX.post_terminal_elapsed(10.0, 2.0, 12.0))


if __name__ == "__main__":
    unittest.main()
