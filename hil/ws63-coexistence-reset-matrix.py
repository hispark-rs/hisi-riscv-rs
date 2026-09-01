#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyserial"]
# ///
"""Run paired WS63 Wi-Fi/BLE and Wi-Fi/SLE coexistence contracts."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import time
from pathlib import Path

import serial

from jlink_nrst import pulse_nrst


COMMON_MARKERS = (
    b"RFDBG_COEX_INIT_BEGIN",
    b"RFDBG_COEX_RF_POWER_OK",
    b"RFDBG_COEX_RTOS_OK",
    b"RFDBG_COEX_INIT_OK",
)
ROLE_MARKERS = {
    "ble": (b"RFDBG_BLE_B1_SHARED_PLATFORM_OK",),
    "sle": (b"RFDBG_SLE_S1_SHARED_PLATFORM_OK",),
    "softap": (b"RFDBG_SOFTAP_READY", b"RFDBG_SOFTAP_NET_READY"),
}
BLE_ACTIVITY_MARKERS = (
    b"RFDBG_COEX_BLE_ADV_ACTIVE",
    b"RFDBG_COEX_WIFI_BLE_ACTIVITY_OK",
)
BLE_SCAN_MARKER = b"RFDBG_COEX_WIFI_SCAN_OK"
BLE_ACTIVITY_SCAN_COUNT = 3
BLE_TRAFFIC_MARKERS = (
    b"RFDBG_COEX_BLE_ADV_ACTIVE",
    b"RFDBG_COEX_WIFI_CONNECT_OK",
    b"RFDBG_COEX_LOCAL_ECHO",
    b"RFDBG_COEX_WIFI_BLE_TRAFFIC_OK",
)
BLE_CONNECTED_TRAFFIC_MARKERS = (
    b"RFDBG_COEX_BLE_CONNECTED",
    b"RFDBG_COEX_WIFI_CONNECT_OK",
    b"RFDBG_COEX_LOCAL_ECHO",
    b"RFDBG_COEX_WIFI_BLE_CONNECTED_TRAFFIC_OK",
)
SLE_TRAFFIC_MARKERS = (
    b"RFDBG_COEX_SLE_ANNOUNCE_ACTIVE",
    b"RFDBG_COEX_WIFI_CONNECT_OK",
    b"RFDBG_COEX_LOCAL_ECHO",
    b"RFDBG_COEX_WIFI_SLE_TRAFFIC_OK",
)
SLE_CONNECTED_TRAFFIC_MARKERS = (
    b"RFDBG_COEX_SLE_CONNECTED",
    b"RFDBG_COEX_WIFI_CONNECT_OK",
    b"RFDBG_COEX_LOCAL_ECHO",
    b"RFDBG_COEX_WIFI_SLE_CONNECTED_TRAFFIC_OK",
)
SOFTAP_SLE_CONNECTED_MARKERS = (
    b"RFDBG_COEX_SLE_SERVER_READY",
    b"RFDBG_COEX_SLE_SERVER_CONNECTED",
)
SOFTAP_BLE_CONNECTED_MARKERS = (
    b"RFDBG_COEX_BLE_SERVER_READY",
    b"RFDBG_COEX_BLE_SERVER_CONNECTED",
)
SOFTAP_TRAFFIC_MARKER = b"RFDBG_SOFTAP_NET"
LOCAL_ECHO_PATTERN = re.compile(
    rb"RFDBG_COEX_LOCAL_ECHO sent=0x([0-9a-fA-F]{8}) "
    rb"received=0x([0-9a-fA-F]{8}) attempts=0x([0-9a-fA-F]{8})"
)
SOFTAP_ECHO_PATTERN = re.compile(
    rb"RFDBG_SOFTAP_NET .*?echo_rx=([0-9a-fA-F]{8}) "
    rb"echo_tx=([0-9a-fA-F]{8})"
)
ACTIVITY_EVENT_PATTERN = re.compile(
    rb"RFDBG_COEX_EVENT_CONSERVATION "
    rb"wifi_accepted=0x([0-9a-fA-F]{8}) wifi_consumed=0x([0-9a-fA-F]{8}) "
    rb"wifi_pending=0x([0-9a-fA-F]{8}) wifi_dropped=0x([0-9a-fA-F]{8}) "
    rb"wifi_high_water=0x([0-9a-fA-F]{8}) "
    rb"protocol_accepted=0x([0-9a-fA-F]{8}) "
    rb"protocol_consumed=0x([0-9a-fA-F]{8}) "
    rb"protocol_pending=0x([0-9a-fA-F]{8}) "
    rb"protocol_dropped=0x([0-9a-fA-F]{8}) "
    rb"protocol_high_water=0x([0-9a-fA-F]{8})"
)
SERVER_EVENT_PATTERN = re.compile(
    rb"RFDBG_COEX_SERVER_EVENT_CONSERVATION accepted=0x([0-9a-fA-F]{8}) "
    rb"consumed=0x([0-9a-fA-F]{8}) pending=0x([0-9a-fA-F]{8}) "
    rb"dropped=0x([0-9a-fA-F]{8}) high_water=0x([0-9a-fA-F]{8})"
)
ACTIVITY_RESOURCE_PATTERN = re.compile(
    rb"RFDBG_COEX_RESOURCE_ACCEPTANCE arena=0x([0-9a-fA-F]{8}) "
    rb"free=0x([0-9a-fA-F]{8}) peak=0x([0-9a-fA-F]{8}) "
    rb"failures=0x([0-9a-fA-F]{8}) min_free=0x([0-9a-fA-F]{8}) "
    rb"max_ready_ms=0x([0-9a-fA-F]{8}) ready_limit_ms=0x([0-9a-fA-F]{8}) "
    rb"max_irq_ms=0x([0-9a-fA-F]{8}) irq_limit_ms=0x([0-9a-fA-F]{8}) "
    rb"ready_owner_err=0x([0-9a-fA-F]{8}) ready_dup=0x([0-9a-fA-F]{8}) "
    rb"ready_wrong_bucket=0x([0-9a-fA-F]{8}) ready_bad_link=0x([0-9a-fA-F]{8})"
)
SERVER_RESOURCE_PATTERN = re.compile(
    rb"RFDBG_COEX_SERVER_RESOURCE_ACCEPTANCE arena=0x([0-9a-fA-F]{8}) "
    rb"free=0x([0-9a-fA-F]{8}) peak=0x([0-9a-fA-F]{8}) "
    rb"failures=0x([0-9a-fA-F]{8}) min_free=0x([0-9a-fA-F]{8}) "
    rb"max_ready_ms=0x([0-9a-fA-F]{8}) ready_limit_ms=0x([0-9a-fA-F]{8}) "
    rb"max_irq_ms=0x([0-9a-fA-F]{8}) irq_limit_ms=0x([0-9a-fA-F]{8}) "
    rb"ready_owner_err=0x([0-9a-fA-F]{8}) ready_dup=0x([0-9a-fA-F]{8}) "
    rb"ready_wrong_bucket=0x([0-9a-fA-F]{8}) ready_bad_link=0x([0-9a-fA-F]{8})"
)
FAILURE_MARKERS = (
    b"panicked at",
    b"RFDBG_MISSING_ROM_CALLBACK",
    b"RFDBG_COEX_INIT_ERR",
    b"RFDBG_COEX_WIFI_RUNNER_ERR",
    b"RFDBG_COEX_BLE_ADV_ERR",
    b"RFDBG_COEX_BLE_CONNECT_ERR",
    b"RFDBG_COEX_BLE_EVENT_DROP",
    b"RFDBG_COEX_BLE_SERVER_ERR",
    b"RFDBG_COEX_BLE_SERVER_EVENT_DROP",
    b"RFDBG_COEX_SLE_ANNOUNCE_ERR",
    b"RFDBG_COEX_SLE_EVENT_DROP",
    b"RFDBG_COEX_SLE_SERVER_ERR",
    b"RFDBG_COEX_SLE_SERVER_EVENT_DROP",
    b"RFDBG_COEX_WIFI_INITIALIZE_ERR",
    b"RFDBG_COEX_WIFI_SCAN_ERR",
    b"RFDBG_COEX_WIFI_CONNECT_ERR",
    b"RFDBG_COEX_LOCAL_ECHO_ERR",
    b"RFDBG_COEX_EVENT_ERR",
    b"RFDBG_COEX_ACCEPTANCE_ERR",
    b"RFDBG_SOFTAP_NET_ERR",
    b"scheduler contract violation",
)
CONTRACT_NAMES = {
    "shared-init": "ws63-wifi-bgle-shared-init/v1",
    "ble-activity": "ws63-wifi-ble-activity/v1",
    "wifi-ble-traffic": "ws63-wifi-ble-local-traffic/v1",
    "wifi-ble-connected-traffic": "ws63-wifi-ble-connected-local-traffic/v1",
    "wifi-sle-traffic": "ws63-wifi-sle-local-traffic/v1",
    "wifi-sle-connected-traffic": "ws63-wifi-sle-connected-local-traffic/v1",
}
TRAFFIC_CONTRACTS = (
    "wifi-ble-traffic",
    "wifi-ble-connected-traffic",
    "wifi-sle-traffic",
    "wifi-sle-connected-traffic",
)
ACCEPTANCE_CONTRACTS = (
    "wifi-ble-connected-traffic",
    "wifi-sle-connected-traffic",
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def drain(port: serial.Serial) -> bytes:
    waiting = port.in_waiting
    return port.read(waiting) if waiting else b""


def endpoint_roles(contract: str) -> tuple[str, str]:
    if contract == "wifi-ble-traffic":
        return "ble", "softap"
    if contract == "wifi-ble-connected-traffic":
        return "softap", "ble"
    if contract in ("wifi-sle-traffic", "wifi-sle-connected-traffic"):
        return "softap", "sle"
    return "ble", "sle"


def traffic_activity_role(contract: str) -> str | None:
    if contract in ("wifi-ble-traffic", "wifi-ble-connected-traffic"):
        return "ble"
    if contract in ("wifi-sle-traffic", "wifi-sle-connected-traffic"):
        return "sle"
    return None


def required_markers(contract: str, role: str) -> tuple[bytes, ...]:
    if contract in TRAFFIC_CONTRACTS and role == "softap":
        required = ROLE_MARKERS[role] + (SOFTAP_TRAFFIC_MARKER,)
        if contract == "wifi-ble-connected-traffic":
            required += SOFTAP_BLE_CONNECTED_MARKERS
        elif contract == "wifi-sle-connected-traffic":
            required += SOFTAP_SLE_CONNECTED_MARKERS
        return required
    required = COMMON_MARKERS + ROLE_MARKERS[role]
    if contract == "ble-activity" and role == "ble":
        required += BLE_ACTIVITY_MARKERS
    elif contract == "wifi-ble-traffic" and role == "ble":
        required += BLE_TRAFFIC_MARKERS
    elif contract == "wifi-ble-connected-traffic" and role == "ble":
        required += BLE_CONNECTED_TRAFFIC_MARKERS
    elif contract == "wifi-sle-traffic" and role == "sle":
        required += SLE_TRAFFIC_MARKERS
    elif contract == "wifi-sle-connected-traffic" and role == "sle":
        required += SLE_CONNECTED_TRAFFIC_MARKERS
    return required


def completion_marker(contract: str, role: str) -> bytes:
    if contract == "wifi-ble-traffic":
        return BLE_TRAFFIC_MARKERS[-1] if role == "ble" else SOFTAP_TRAFFIC_MARKER
    if contract == "wifi-ble-connected-traffic":
        return (
            BLE_CONNECTED_TRAFFIC_MARKERS[-1]
            if role == "ble"
            else SOFTAP_TRAFFIC_MARKER
        )
    if contract == "wifi-sle-traffic":
        return SLE_TRAFFIC_MARKERS[-1] if role == "sle" else SOFTAP_TRAFFIC_MARKER
    if contract == "wifi-sle-connected-traffic":
        return (
            SLE_CONNECTED_TRAFFIC_MARKERS[-1]
            if role == "sle"
            else SOFTAP_TRAFFIC_MARKER
        )
    if contract == "ble-activity" and role == "ble":
        return BLE_ACTIVITY_MARKERS[-1]
    return COMMON_MARKERS[-1]


def classify(contract: str, role: str, payload: bytes) -> dict[str, object]:
    required = required_markers(contract, role)
    missing = [marker.decode() for marker in required if marker not in payload]
    failures = [marker.decode() for marker in FAILURE_MARKERS if marker in payload]
    activity_role = traffic_activity_role(contract)
    scan_count = payload.count(BLE_SCAN_MARKER) if role in ("ble", activity_role) else 0
    local_echo: dict[str, int] | None = None
    softap_echo: dict[str, int] | None = None
    event_conservation: dict[str, int] | None = None
    resource_acceptance: dict[str, int] | None = None
    if (
        (
            (contract == "ble-activity" and role == "ble")
            or (contract in TRAFFIC_CONTRACTS and role == activity_role)
        )
        and scan_count != BLE_ACTIVITY_SCAN_COUNT
    ):
        missing.append(
            f"{BLE_SCAN_MARKER.decode()} x{BLE_ACTIVITY_SCAN_COUNT}"
            f" (observed {scan_count})"
        )
    if contract in TRAFFIC_CONTRACTS and role == activity_role:
        matches = LOCAL_ECHO_PATTERN.findall(payload)
        if matches:
            sent, received, attempts = (int(value, 16) for value in matches[-1])
            local_echo = {"sent": sent, "received": received, "attempts": attempts}
            if sent != 10 or received != 10 or not 10 <= attempts <= 30:
                missing.append(
                    "local echo requires sent=10 received=10 and 10<=attempts<=30"
                )
        else:
            missing.append("parseable RFDBG_COEX_LOCAL_ECHO")
    elif contract in TRAFFIC_CONTRACTS and role == "softap":
        matches = SOFTAP_ECHO_PATTERN.findall(payload)
        if matches:
            values = [(int(a, 16), int(b, 16)) for a, b in matches]
            rx = max(value[0] for value in values)
            tx = max(value[1] for value in values)
            softap_echo = {"received": rx, "sent": tx}
            if rx < 10 or tx < 10:
                missing.append("SoftAP echo requires received>=10 and sent>=10")
        else:
            missing.append("parseable RFDBG_SOFTAP_NET echo counters")
    if contract in ACCEPTANCE_CONTRACTS:
        if role == activity_role:
            event_conservation = parse_activity_events(payload, missing)
            resource_acceptance = parse_resource_acceptance(
                ACTIVITY_RESOURCE_PATTERN, payload, missing, "activity"
            )
        elif role == "softap":
            event_conservation = parse_server_events(payload, missing)
            resource_acceptance = parse_resource_acceptance(
                SERVER_RESOURCE_PATTERN, payload, missing, "server"
            )
    return {
        "pass": not missing and not failures,
        "missing": missing,
        "failure_markers": failures,
        "wifi_scan_ok_count": scan_count,
        "local_echo": local_echo,
        "softap_echo": softap_echo,
        "event_conservation": event_conservation,
        "resource_acceptance": resource_acceptance,
        "bytes": len(payload),
    }


def parse_activity_events(payload: bytes, missing: list[str]) -> dict[str, int] | None:
    matches = ACTIVITY_EVENT_PATTERN.findall(payload)
    if not matches:
        missing.append("parseable RFDBG_COEX_EVENT_CONSERVATION")
        return None
    names = (
        "wifi_accepted",
        "wifi_consumed",
        "wifi_pending",
        "wifi_dropped",
        "wifi_high_water",
        "protocol_accepted",
        "protocol_consumed",
        "protocol_pending",
        "protocol_dropped",
        "protocol_high_water",
    )
    values = dict(zip(names, (int(value, 16) for value in matches[-1]), strict=True))
    if (
        values["wifi_accepted"]
        != values["wifi_consumed"] + values["wifi_pending"]
        or values["protocol_accepted"]
        != values["protocol_consumed"] + values["protocol_pending"]
        or values["wifi_dropped"] != 0
        or values["protocol_dropped"] != 0
        or values["wifi_pending"] > 8
        or values["wifi_high_water"] > 8
        or values["protocol_pending"] > 32
        or values["protocol_high_water"] > 32
    ):
        missing.append("activity event conservation requires zero drops and bounded ownership")
    return values


def parse_server_events(payload: bytes, missing: list[str]) -> dict[str, int] | None:
    matches = SERVER_EVENT_PATTERN.findall(payload)
    if not matches:
        missing.append("parseable RFDBG_COEX_SERVER_EVENT_CONSERVATION")
        return None
    names = ("accepted", "consumed", "pending", "dropped", "high_water")
    values = dict(zip(names, (int(value, 16) for value in matches[-1]), strict=True))
    if (
        values["accepted"] != values["consumed"] + values["pending"]
        or values["dropped"] != 0
        or values["pending"] > 32
        or values["high_water"] > 32
    ):
        missing.append("server event conservation requires zero drops and bounded ownership")
    return values


def parse_resource_acceptance(
    pattern: re.Pattern[bytes], payload: bytes, missing: list[str], owner: str
) -> dict[str, int] | None:
    matches = pattern.findall(payload)
    if not matches:
        missing.append(f"parseable {owner} coexistence resource acceptance")
        return None
    names = (
        "arena",
        "free",
        "peak",
        "failures",
        "min_free",
        "max_ready_ms",
        "ready_limit_ms",
        "max_irq_ms",
        "irq_limit_ms",
        "ready_owner_err",
        "ready_dup",
        "ready_wrong_bucket",
        "ready_bad_link",
    )
    values = dict(zip(names, (int(value, 16) for value in matches[-1]), strict=True))
    if (
        values["arena"] == 0
        or values["peak"] > values["arena"]
        or values["free"] < values["min_free"]
        or values["failures"] != 0
        or values["max_ready_ms"] > values["ready_limit_ms"]
        or values["max_irq_ms"] > values["irq_limit_ms"]
        or any(
            values[name] != 0
            for name in (
                "ready_owner_err",
                "ready_dup",
                "ready_wrong_bucket",
                "ready_bad_link",
            )
        )
    ):
        missing.append(f"{owner} resource/latency acceptance exceeded its fixture contract")
    return values


def summarize_acceptance(records: list[dict[str, object]]) -> dict[str, int] | None:
    event_snapshots: list[dict[str, int]] = []
    resource_snapshots: list[dict[str, int]] = []
    for record in records:
        for endpoint in ("ble", "sle"):
            result = record[endpoint]
            assert isinstance(result, dict)
            events = result.get("event_conservation")
            resources = result.get("resource_acceptance")
            if isinstance(events, dict):
                event_snapshots.append(events)
            if isinstance(resources, dict):
                resource_snapshots.append(resources)
    if not event_snapshots or not resource_snapshots:
        return None

    dropped_total = 0
    high_water = 0
    for events in event_snapshots:
        dropped_total += events.get("dropped", 0)
        dropped_total += events.get("wifi_dropped", 0)
        dropped_total += events.get("protocol_dropped", 0)
        high_water = max(
            high_water,
            events.get("high_water", 0),
            events.get("wifi_high_water", 0),
            events.get("protocol_high_water", 0),
        )
    return {
        "event_snapshots": len(event_snapshots),
        "event_dropped_total": dropped_total,
        "max_event_high_water": high_water,
        "min_heap_free": min(item["free"] for item in resource_snapshots),
        "max_heap_peak": max(item["peak"] for item in resource_snapshots),
        "allocation_failures_total": sum(
            item["failures"] for item in resource_snapshots
        ),
        "max_ready_ms": max(item["max_ready_ms"] for item in resource_snapshots),
        "max_irq_ms": max(item["max_irq_ms"] for item in resource_snapshots),
    }


def capture_pair(
    ble: serial.Serial,
    peer: serial.Serial,
    ble_jlink: str,
    peer_jlink: str,
    timeout: float,
    contract: str,
) -> tuple[bytes, bytes]:
    ble.reset_input_buffer()
    peer.reset_input_buffer()
    pulse_nrst("JLinkExe", ble_jlink)
    pulse_nrst("JLinkExe", peer_jlink)

    ble_log = bytearray()
    peer_log = bytearray()
    complete_at: float | None = None
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        ble_log.extend(drain(ble))
        peer_log.extend(drain(peer))
        first_role, second_role = endpoint_roles(contract)
        if contract in TRAFFIC_CONTRACTS:
            both_complete = (
                classify(contract, first_role, bytes(ble_log))["pass"] is True
                and classify(contract, second_role, bytes(peer_log))["pass"] is True
            )
        else:
            both_complete = completion_marker(
                contract, first_role
            ) in ble_log and completion_marker(contract, second_role) in peer_log
        if both_complete and complete_at is None:
            complete_at = time.monotonic()
        if complete_at is not None and time.monotonic() - complete_at >= 0.5:
            break
        time.sleep(0.01)

    ble_log.extend(drain(ble))
    peer_log.extend(drain(peer))
    return bytes(ble_log), bytes(peer_log)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ble-port", required=True)
    parser.add_argument("--ble-jlink", required=True)
    parser.add_argument("--ble-elf", type=Path, required=True)
    parser.add_argument("--sle-port", required=True)
    parser.add_argument("--sle-jlink", required=True)
    parser.add_argument("--sle-elf", type=Path, required=True)
    parser.add_argument("--baud", type=int, default=115_200)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument(
        "--contract",
        choices=tuple(CONTRACT_NAMES),
        default="shared-init",
        help="coexistence behavior contract to verify",
    )
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.runs <= 0 or args.timeout <= 0:
        raise SystemExit("--runs and --timeout must be positive")
    for elf in (args.ble_elf, args.sle_elf):
        if not elf.is_file():
            raise SystemExit(f"ELF not found: {elf}")
    args.output.mkdir(parents=True, exist_ok=False)

    records: list[dict[str, object]] = []
    with serial.Serial(args.ble_port, args.baud, timeout=0) as ble:
        with serial.Serial(args.sle_port, args.baud, timeout=0) as sle:
            first_role, second_role = endpoint_roles(args.contract)
            for run in range(1, args.runs + 1):
                ble_log, sle_log = capture_pair(
                    ble,
                    sle,
                    args.ble_jlink,
                    args.sle_jlink,
                    args.timeout,
                    args.contract,
                )
                ble_name = f"run-{run:02d}.ble.uart.log"
                sle_name = f"run-{run:02d}.sle.uart.log"
                (args.output / ble_name).write_bytes(ble_log)
                (args.output / sle_name).write_bytes(sle_log)
                ble_result = classify(args.contract, first_role, ble_log)
                sle_result = classify(args.contract, second_role, sle_log)
                passed = ble_result["pass"] is True and sle_result["pass"] is True
                records.append(
                    {
                        "run": run,
                        "pass": passed,
                        "ble": {**ble_result, "role": first_role, "log": ble_name},
                        "sle": {**sle_result, "role": second_role, "log": sle_name},
                    }
                )
                print(
                    f"run {run:02d}/{args.runs}: {'pass' if passed else 'fail'}",
                    flush=True,
                )

    passed = sum(record["pass"] is True for record in records)
    summary = {
        "schema_version": 3,
        "contract": CONTRACT_NAMES[args.contract],
        "runs": args.runs,
        "passed": passed,
        "failed": args.runs - passed,
        "ble": {
            "role": first_role,
            "port": args.ble_port,
            "jlink": args.ble_jlink,
            "elf": str(args.ble_elf),
            "elf_sha256": sha256(args.ble_elf),
        },
        "sle": {
            "role": second_role,
            "port": args.sle_port,
            "jlink": args.sle_jlink,
            "elf": str(args.sle_elf),
            "elf_sha256": sha256(args.sle_elf),
        },
        "acceptance": summarize_acceptance(records),
        "records": records,
    }
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"summary: {passed}/{args.runs} pass")
    print(f"artifacts: {args.output}")
    return 0 if passed == args.runs else 1


if __name__ == "__main__":
    raise SystemExit(main())
