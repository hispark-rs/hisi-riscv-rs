#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyserial"]
# ///
"""Host tests for paired WS63 coexistence marker contracts."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("ws63-coexistence-reset-matrix.py")
SPEC = importlib.util.spec_from_file_location("ws63_coexistence_matrix", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MATRIX = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MATRIX)


def complete_log(contract: str, role: str) -> bytes:
    markers = list(MATRIX.required_markers(contract, role))
    activity_role = MATRIX.traffic_activity_role(contract)
    if (contract == "ble-activity" and role == "ble") or (
        contract in MATRIX.TRAFFIC_CONTRACTS and role == activity_role
    ):
        markers.extend([MATRIX.BLE_SCAN_MARKER] * MATRIX.BLE_ACTIVITY_SCAN_COUNT)
    if contract in MATRIX.TRAFFIC_CONTRACTS and role == activity_role:
        markers.append(
            b"RFDBG_COEX_LOCAL_ECHO sent=0x0000000a received=0x0000000a "
            b"attempts=0x0000000a bitmap=0x000003ff"
        )
    elif contract in MATRIX.TRAFFIC_CONTRACTS and role == "softap":
        markers.append(b"RFDBG_SOFTAP_NET echo_rx=0000000a echo_tx=0000000a")
    if contract in MATRIX.ACCEPTANCE_CONTRACTS and role == activity_role:
        markers.append(
            b"RFDBG_COEX_EVENT_CONSERVATION "
            b"wifi_accepted=0x00000004 wifi_consumed=0x00000004 "
            b"wifi_pending=0x00000000 wifi_dropped=0x00000000 "
            b"wifi_high_water=0x00000001 protocol_accepted=0x00000008 "
            b"protocol_consumed=0x00000008 protocol_pending=0x00000000 "
            b"protocol_dropped=0x00000000 protocol_high_water=0x00000002"
        )
        markers.append(
            b"RFDBG_COEX_RESOURCE_ACCEPTANCE arena=0x000115c0 free=0x000066e0 "
            b"peak=0x0000aee0 failures=0x00000000 min_free=0x00004000 "
            b"max_ready_ms=0x00000100 ready_limit_ms=0x000007d0 "
            b"max_irq_ms=0x00000008 irq_limit_ms=0x00000064 "
            b"ready_owner_err=0x00000000 ready_dup=0x00000000 "
            b"ready_wrong_bucket=0x00000000 ready_bad_link=0x00000000"
        )
    elif contract in MATRIX.ACCEPTANCE_CONTRACTS and role == "softap":
        markers.append(
            b"RFDBG_COEX_SERVER_EVENT_CONSERVATION accepted=0x00000008 "
            b"consumed=0x00000008 pending=0x00000000 dropped=0x00000000 "
            b"high_water=0x00000002"
        )
        markers.append(
            b"RFDBG_COEX_SERVER_RESOURCE_ACCEPTANCE arena=0x000115c0 "
            b"free=0x000066e0 peak=0x0000aee0 failures=0x00000000 "
            b"min_free=0x00004000 max_ready_ms=0x00000100 "
            b"ready_limit_ms=0x000007d0 max_irq_ms=0x00000008 "
            b"irq_limit_ms=0x00000064 ready_owner_err=0x00000000 "
            b"ready_dup=0x00000000 ready_wrong_bucket=0x00000000 "
            b"ready_bad_link=0x00000000"
        )
    return b"\n".join(markers)


class ClassifyTests(unittest.TestCase):
    def test_shared_init_contract_is_backward_compatible(self) -> None:
        for role in ("ble", "sle"):
            with self.subTest(role=role):
                result = MATRIX.classify(
                    "shared-init", role, complete_log("shared-init", role)
                )
                self.assertTrue(result["pass"])

    def test_ble_activity_requires_three_scans(self) -> None:
        result = MATRIX.classify(
            "ble-activity", "ble", complete_log("ble-activity", "ble")
        )
        self.assertTrue(result["pass"])
        self.assertEqual(result["wifi_scan_ok_count"], 3)

    def test_ble_activity_fails_when_scan_count_is_short(self) -> None:
        payload = complete_log("ble-activity", "ble").replace(
            MATRIX.BLE_SCAN_MARKER + b"\n", b"", 1
        )
        result = MATRIX.classify("ble-activity", "ble", payload)
        self.assertFalse(result["pass"])
        self.assertEqual(result["wifi_scan_ok_count"], 2)
        self.assertIn("observed 2", result["missing"][-1])

    def test_ble_activity_fails_closed_on_event_drop(self) -> None:
        payload = complete_log("ble-activity", "ble")
        payload += b"\nRFDBG_COEX_BLE_EVENT_DROP"
        result = MATRIX.classify("ble-activity", "ble", payload)
        self.assertFalse(result["pass"])
        self.assertEqual(
            result["failure_markers"], ["RFDBG_COEX_BLE_EVENT_DROP"]
        )

    def test_sle_peer_remains_shared_init_control(self) -> None:
        result = MATRIX.classify(
            "ble-activity", "sle", complete_log("ble-activity", "sle")
        )
        self.assertTrue(result["pass"])
        self.assertEqual(result["wifi_scan_ok_count"], 0)

    def test_wifi_ble_traffic_requires_local_echo_and_softap_counts(self) -> None:
        activity = MATRIX.classify(
            "wifi-ble-traffic", "ble", complete_log("wifi-ble-traffic", "ble")
        )
        peer = MATRIX.classify(
            "wifi-ble-traffic",
            "softap",
            complete_log("wifi-ble-traffic", "softap"),
        )
        self.assertTrue(activity["pass"])
        self.assertTrue(peer["pass"])
        self.assertEqual(activity["wifi_scan_ok_count"], 3)

    def test_wifi_ble_traffic_fails_closed_on_short_echo(self) -> None:
        payload = complete_log("wifi-ble-traffic", "ble").replace(
            b"received=0x0000000a", b"received=0x00000009"
        )
        result = MATRIX.classify("wifi-ble-traffic", "ble", payload)
        self.assertFalse(result["pass"])

    def test_wifi_ble_traffic_accepts_bounded_retry(self) -> None:
        payload = complete_log("wifi-ble-traffic", "ble").replace(
            b"attempts=0x0000000a", b"attempts=0x0000000b"
        )
        result = MATRIX.classify("wifi-ble-traffic", "ble", payload)
        self.assertTrue(result["pass"])
        self.assertEqual(result["local_echo"]["attempts"], 11)

    def test_wifi_ble_traffic_fails_closed_on_softap_short_count(self) -> None:
        payload = complete_log("wifi-ble-traffic", "softap").replace(
            b"echo_tx=0000000a", b"echo_tx=00000009"
        )
        result = MATRIX.classify("wifi-ble-traffic", "softap", payload)
        self.assertFalse(result["pass"])

    def test_wifi_ble_traffic_uses_independent_softap_counter_maxima(self) -> None:
        payload = complete_log("wifi-ble-traffic", "softap")
        payload += b"\nRFDBG_SOFTAP_NET echo_rx=0000000b echo_tx=00000009"
        result = MATRIX.classify("wifi-ble-traffic", "softap", payload)
        self.assertTrue(result["pass"])
        self.assertEqual(result["softap_echo"], {"received": 11, "sent": 10})

    def test_wifi_ble_connected_traffic_requires_link_and_echo_on_both_boards(
        self,
    ) -> None:
        contract = "wifi-ble-connected-traffic"
        softap = MATRIX.classify(contract, "softap", complete_log(contract, "softap"))
        activity = MATRIX.classify(contract, "ble", complete_log(contract, "ble"))
        self.assertTrue(softap["pass"])
        self.assertTrue(activity["pass"])
        self.assertEqual(activity["wifi_scan_ok_count"], 3)
        self.assertEqual(MATRIX.endpoint_roles(contract), ("softap", "ble"))

    def test_wifi_ble_connected_traffic_fails_without_server_connection(self) -> None:
        contract = "wifi-ble-connected-traffic"
        payload = complete_log(contract, "softap").replace(
            b"RFDBG_COEX_BLE_SERVER_CONNECTED\n", b"", 1
        )
        result = MATRIX.classify(contract, "softap", payload)
        self.assertFalse(result["pass"])
        self.assertIn("RFDBG_COEX_BLE_SERVER_CONNECTED", result["missing"])

    def test_wifi_ble_connected_traffic_fails_closed_on_client_error(self) -> None:
        contract = "wifi-ble-connected-traffic"
        payload = complete_log(contract, "ble")
        payload += b"\nRFDBG_COEX_BLE_CONNECT_ERR stage=connect"
        result = MATRIX.classify(contract, "ble", payload)
        self.assertFalse(result["pass"])
        self.assertEqual(
            result["failure_markers"], ["RFDBG_COEX_BLE_CONNECT_ERR"]
        )

    def test_connected_traffic_requires_event_conservation(self) -> None:
        contract = "wifi-ble-connected-traffic"
        payload = complete_log(contract, "ble").replace(
            b"wifi_accepted=0x00000004", b"wifi_accepted=0x00000005"
        )
        result = MATRIX.classify(contract, "ble", payload)
        self.assertFalse(result["pass"])
        self.assertIn("event conservation", result["missing"][-1])

    def test_connected_traffic_requires_resource_headroom(self) -> None:
        contract = "wifi-sle-connected-traffic"
        payload = complete_log(contract, "sle").replace(
            b"free=0x000066e0", b"free=0x00002000"
        )
        result = MATRIX.classify(contract, "sle", payload)
        self.assertFalse(result["pass"])
        self.assertIn("resource/latency", result["missing"][-1])

    def test_connected_softap_requires_server_acceptance_markers(self) -> None:
        contract = "wifi-ble-connected-traffic"
        payload = complete_log(contract, "softap").replace(
            b"RFDBG_COEX_SERVER_EVENT_CONSERVATION", b"RFDBG_COEX_SERVER_EVENT_MISSING"
        )
        result = MATRIX.classify(contract, "softap", payload)
        self.assertFalse(result["pass"])
        self.assertTrue(
            any("SERVER_EVENT_CONSERVATION" in item for item in result["missing"])
        )

    def test_acceptance_summary_aggregates_both_endpoints(self) -> None:
        contract = "wifi-ble-connected-traffic"
        records = [
            {
                "ble": MATRIX.classify(contract, "softap", complete_log(contract, "softap")),
                "sle": MATRIX.classify(contract, "ble", complete_log(contract, "ble")),
            }
        ]
        summary = MATRIX.summarize_acceptance(records)
        self.assertIsNotNone(summary)
        assert summary is not None
        self.assertEqual(summary["event_snapshots"], 2)
        self.assertEqual(summary["event_dropped_total"], 0)
        self.assertEqual(summary["min_heap_free"], 0x66E0)

    def test_wifi_sle_traffic_requires_announce_echo_and_softap_counts(self) -> None:
        softap = MATRIX.classify(
            "wifi-sle-traffic",
            "softap",
            complete_log("wifi-sle-traffic", "softap"),
        )
        activity = MATRIX.classify(
            "wifi-sle-traffic", "sle", complete_log("wifi-sle-traffic", "sle")
        )
        self.assertTrue(softap["pass"])
        self.assertTrue(activity["pass"])
        self.assertEqual(activity["wifi_scan_ok_count"], 3)

    def test_wifi_sle_traffic_fails_closed_on_announce_error(self) -> None:
        payload = complete_log("wifi-sle-traffic", "sle")
        payload += b"\nRFDBG_COEX_SLE_ANNOUNCE_ERR"
        result = MATRIX.classify("wifi-sle-traffic", "sle", payload)
        self.assertFalse(result["pass"])
        self.assertEqual(
            result["failure_markers"], ["RFDBG_COEX_SLE_ANNOUNCE_ERR"]
        )

    def test_wifi_sle_traffic_maps_physical_endpoints_explicitly(self) -> None:
        self.assertEqual(
            MATRIX.endpoint_roles("wifi-sle-traffic"), ("softap", "sle")
        )

    def test_wifi_sle_connected_traffic_requires_link_and_echo_on_both_boards(
        self,
    ) -> None:
        contract = "wifi-sle-connected-traffic"
        softap = MATRIX.classify(contract, "softap", complete_log(contract, "softap"))
        activity = MATRIX.classify(contract, "sle", complete_log(contract, "sle"))
        self.assertTrue(softap["pass"])
        self.assertTrue(activity["pass"])
        self.assertEqual(activity["wifi_scan_ok_count"], 3)
        self.assertEqual(MATRIX.endpoint_roles(contract), ("softap", "sle"))

    def test_wifi_sle_connected_traffic_fails_without_server_connection(self) -> None:
        contract = "wifi-sle-connected-traffic"
        payload = complete_log(contract, "softap").replace(
            b"RFDBG_COEX_SLE_SERVER_CONNECTED\n", b"", 1
        )
        result = MATRIX.classify(contract, "softap", payload)
        self.assertFalse(result["pass"])
        self.assertIn("RFDBG_COEX_SLE_SERVER_CONNECTED", result["missing"])

    def test_wifi_sle_connected_traffic_fails_closed_on_server_event_drop(
        self,
    ) -> None:
        contract = "wifi-sle-connected-traffic"
        payload = complete_log(contract, "softap")
        payload += b"\nRFDBG_COEX_SLE_SERVER_EVENT_DROP"
        result = MATRIX.classify(contract, "softap", payload)
        self.assertFalse(result["pass"])
        self.assertEqual(
            result["failure_markers"], ["RFDBG_COEX_SLE_SERVER_EVENT_DROP"]
        )


if __name__ == "__main__":
    unittest.main()
