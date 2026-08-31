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
SOFTAP_TRAFFIC_MARKER = b"RFDBG_SOFTAP_NET"
LOCAL_ECHO_PATTERN = re.compile(
    rb"RFDBG_COEX_LOCAL_ECHO sent=0x([0-9a-fA-F]{8}) "
    rb"received=0x([0-9a-fA-F]{8}) attempts=0x([0-9a-fA-F]{8})"
)
SOFTAP_ECHO_PATTERN = re.compile(
    rb"RFDBG_SOFTAP_NET .*?echo_rx=([0-9a-fA-F]{8}) "
    rb"echo_tx=([0-9a-fA-F]{8})"
)
FAILURE_MARKERS = (
    b"panicked at",
    b"RFDBG_MISSING_ROM_CALLBACK",
    b"RFDBG_COEX_INIT_ERR",
    b"RFDBG_COEX_WIFI_RUNNER_ERR",
    b"RFDBG_COEX_BLE_ADV_ERR",
    b"RFDBG_COEX_BLE_EVENT_DROP",
    b"RFDBG_COEX_WIFI_INITIALIZE_ERR",
    b"RFDBG_COEX_WIFI_SCAN_ERR",
    b"RFDBG_COEX_WIFI_CONNECT_ERR",
    b"RFDBG_COEX_LOCAL_ECHO_ERR",
    b"RFDBG_COEX_EVENT_ERR",
    b"RFDBG_SOFTAP_NET_ERR",
    b"scheduler contract violation",
)
CONTRACT_NAMES = {
    "shared-init": "ws63-wifi-bgle-shared-init/v1",
    "ble-activity": "ws63-wifi-ble-activity/v1",
    "wifi-ble-traffic": "ws63-wifi-ble-local-traffic/v1",
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def drain(port: serial.Serial) -> bytes:
    waiting = port.in_waiting
    return port.read(waiting) if waiting else b""


def required_markers(contract: str, role: str) -> tuple[bytes, ...]:
    if contract == "wifi-ble-traffic" and role == "softap":
        return ROLE_MARKERS[role] + (SOFTAP_TRAFFIC_MARKER,)
    required = COMMON_MARKERS + ROLE_MARKERS[role]
    if contract == "ble-activity" and role == "ble":
        required += BLE_ACTIVITY_MARKERS
    elif contract == "wifi-ble-traffic" and role == "ble":
        required += BLE_TRAFFIC_MARKERS
    return required


def completion_marker(contract: str, role: str) -> bytes:
    if contract == "wifi-ble-traffic":
        return BLE_TRAFFIC_MARKERS[-1] if role == "ble" else SOFTAP_TRAFFIC_MARKER
    if contract == "ble-activity" and role == "ble":
        return BLE_ACTIVITY_MARKERS[-1]
    return COMMON_MARKERS[-1]


def classify(contract: str, role: str, payload: bytes) -> dict[str, object]:
    required = required_markers(contract, role)
    missing = [marker.decode() for marker in required if marker not in payload]
    failures = [marker.decode() for marker in FAILURE_MARKERS if marker in payload]
    scan_count = payload.count(BLE_SCAN_MARKER) if role == "ble" else 0
    local_echo: dict[str, int] | None = None
    softap_echo: dict[str, int] | None = None
    if (
        contract in ("ble-activity", "wifi-ble-traffic")
        and role == "ble"
        and scan_count != BLE_ACTIVITY_SCAN_COUNT
    ):
        missing.append(
            f"{BLE_SCAN_MARKER.decode()} x{BLE_ACTIVITY_SCAN_COUNT}"
            f" (observed {scan_count})"
        )
    if contract == "wifi-ble-traffic" and role == "ble":
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
    elif contract == "wifi-ble-traffic" and role == "softap":
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
    return {
        "pass": not missing and not failures,
        "missing": missing,
        "failure_markers": failures,
        "wifi_scan_ok_count": scan_count,
        "local_echo": local_echo,
        "softap_echo": softap_echo,
        "bytes": len(payload),
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
        peer_role = "softap" if contract == "wifi-ble-traffic" else "sle"
        if contract == "wifi-ble-traffic":
            both_complete = (
                completion_marker(contract, "ble") in ble_log
                and classify(contract, peer_role, bytes(peer_log))["pass"] is True
            )
        else:
            both_complete = completion_marker(
                contract, "ble"
            ) in ble_log and completion_marker(contract, peer_role) in peer_log
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
            peer_role = "softap" if args.contract == "wifi-ble-traffic" else "sle"
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
                ble_result = classify(args.contract, "ble", ble_log)
                sle_result = classify(args.contract, peer_role, sle_log)
                passed = ble_result["pass"] is True and sle_result["pass"] is True
                records.append(
                    {
                        "run": run,
                        "pass": passed,
                        "ble": {**ble_result, "role": "ble", "log": ble_name},
                        "sle": {**sle_result, "role": peer_role, "log": sle_name},
                    }
                )
                print(
                    f"run {run:02d}/{args.runs}: {'pass' if passed else 'fail'}",
                    flush=True,
                )

    passed = sum(record["pass"] is True for record in records)
    summary = {
        "schema_version": 2,
        "contract": CONTRACT_NAMES[args.contract],
        "runs": args.runs,
        "passed": passed,
        "failed": args.runs - passed,
        "ble": {
            "role": "ble",
            "port": args.ble_port,
            "jlink": args.ble_jlink,
            "elf": str(args.ble_elf),
            "elf_sha256": sha256(args.ble_elf),
        },
        "sle": {
            "role": peer_role,
            "port": args.sle_port,
            "jlink": args.sle_jlink,
            "elf": str(args.sle_elf),
            "elf_sha256": sha256(args.sle_elf),
        },
        "records": records,
    }
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"summary: {passed}/{args.runs} pass")
    print(f"artifacts: {args.output}")
    return 0 if passed == args.runs else 1


if __name__ == "__main__":
    raise SystemExit(main())
