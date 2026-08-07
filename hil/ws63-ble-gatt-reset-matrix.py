#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Run the paired-board WS63 BLE B3 GATT reset matrix."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

import serial

from jlink_nrst import pulse_nrst


SERVER_MARKERS = (
    b"RFDBG_BLE_B3_INIT_OK",
    b"RFDBG_BLE_B3_SERVICE_READY",
    b"RFDBG_BLE_B3_SERVER_ADV_OK",
    b"RFDBG_BLE_B3_SERVER_CONNECTED",
    b"RFDBG_BLE_B3_NOTIFY_SENT",
    b"RFDBG_BLE_B3_INDICATE_SENT",
    b"RFDBG_BLE_B3_INDICATE_CONFIRMED",
    b"RFDBG_BLE_B3_SERVER_DISCONNECTED",
)
CLIENT_MARKERS = (
    b"RFDBG_BLE_B3_INIT_OK",
    b"RFDBG_BLE_B3_CLIENT_SCAN_READY",
    b"RFDBG_BLE_B3_SCAN_MATCH",
    b"RFDBG_BLE_B3_CLIENT_CONNECTED",
    b"RFDBG_BLE_B3_SERVICE_FOUND",
    b"RFDBG_BLE_B3_CHARACTERISTIC_FOUND",
    b"RFDBG_BLE_B3_DESCRIPTOR_FOUND",
    b"RFDBG_BLE_B3_NOTIFICATION_OK",
    b"RFDBG_BLE_B3_INDICATION_OK",
    b"RFDBG_BLE_B3_CLIENT_DISCONNECTED",
    b"RFDBG_BLE_B3_GATT_OK",
)
FAILURE_MARKERS = (
    b"RFDBG_MISSING_ROM_CALLBACK",
    b"RFDBG_BLE_B3_INIT_ERR",
    b"RFDBG_BLE_B3_EVENT_DROP",
    b"RFDBG_BLE_B3_SERVER_REGISTER_ERR",
    b"RFDBG_BLE_B3_SERVER_ADV_ERR",
    b"RFDBG_BLE_B3_SERVER_STATE_ERR",
    b"RFDBG_BLE_B3_NOTIFY_ERR",
    b"RFDBG_BLE_B3_INDICATE_ERR",
    b"RFDBG_BLE_B3_CLIENT_REGISTER_ERR",
    b"RFDBG_BLE_B3_CLIENT_SCAN_ERR",
    b"RFDBG_BLE_B3_CONNECT_ERR",
    b"RFDBG_BLE_B3_SERVICE_DISCOVERY_ERR",
    b"RFDBG_BLE_B3_CHAR_DISCOVERY_ERR",
    b"RFDBG_BLE_B3_DESC_DISCOVERY_ERR",
    b"RFDBG_BLE_B3_CCC_NOTIFY_ERR",
    b"RFDBG_BLE_B3_WRITE_NOTIFY_ERR",
    b"RFDBG_BLE_B3_CCC_INDICATE_ERR",
    b"RFDBG_BLE_B3_WRITE_INDICATE_ERR",
    b"RFDBG_BLE_B3_DISCONNECT_ERR",
)


def drain(port: serial.Serial) -> bytes:
    waiting = port.in_waiting
    return port.read(waiting) if waiting else b""


def classify(server: bytes, client: bytes) -> dict[str, object]:
    server_missing = [
        marker.decode() for marker in SERVER_MARKERS if marker not in server
    ]
    client_missing = [
        marker.decode() for marker in CLIENT_MARKERS if marker not in client
    ]
    failures = [
        marker.decode()
        for marker in FAILURE_MARKERS
        if marker in server or marker in client
    ]
    return {
        "pass": not server_missing and not client_missing and not failures,
        "server_missing": server_missing,
        "client_missing": client_missing,
        "failure_markers": failures,
        "server_bytes": len(server),
        "client_bytes": len(client),
    }


def capture_run(
    server: serial.Serial,
    client: serial.Serial,
    server_jlink: str,
    client_jlink: str,
    server_settle: float,
    timeout: float,
) -> tuple[bytes, bytes]:
    server.reset_input_buffer()
    client.reset_input_buffer()
    pulse_nrst("JLinkExe", server_jlink)
    server_log = bytearray()
    settle_deadline = time.monotonic() + server_settle
    while time.monotonic() < settle_deadline:
        server_log.extend(drain(server))
        time.sleep(0.01)

    pulse_nrst("JLinkExe", client_jlink)
    client_log = bytearray()
    complete_at: float | None = None
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        server_log.extend(drain(server))
        client_log.extend(drain(client))
        complete = (
            CLIENT_MARKERS[-1] in client_log and SERVER_MARKERS[-1] in server_log
        )
        if complete and complete_at is None:
            complete_at = time.monotonic()
        if complete_at is not None and time.monotonic() - complete_at >= 1.0:
            break
        time.sleep(0.01)

    server_log.extend(drain(server))
    client_log.extend(drain(client))
    return bytes(server_log), bytes(client_log)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server-port", required=True)
    parser.add_argument("--server-jlink", required=True)
    parser.add_argument("--client-port", required=True)
    parser.add_argument("--client-jlink", required=True)
    parser.add_argument("--baud", type=int, default=115_200)
    parser.add_argument("--runs", type=int, default=20)
    parser.add_argument("--server-settle", type=float, default=3.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.runs <= 0 or args.server_settle <= 0 or args.timeout <= 0:
        raise SystemExit("--runs, --server-settle and --timeout must be positive")
    args.output.mkdir(parents=True, exist_ok=False)

    records: list[dict[str, object]] = []
    with serial.Serial(args.server_port, args.baud, timeout=0) as server:
        with serial.Serial(args.client_port, args.baud, timeout=0) as client:
            for run in range(1, args.runs + 1):
                server_log, client_log = capture_run(
                    server,
                    client,
                    args.server_jlink,
                    args.client_jlink,
                    args.server_settle,
                    args.timeout,
                )
                server_name = f"run-{run:02d}.server.uart.log"
                client_name = f"run-{run:02d}.client.uart.log"
                (args.output / server_name).write_bytes(server_log)
                (args.output / client_name).write_bytes(client_log)
                record = classify(server_log, client_log)
                record.update(
                    {"run": run, "server_log": server_name, "client_log": client_name}
                )
                records.append(record)
                state = "pass" if record["pass"] else "fail"
                print(f"run {run:02d}/{args.runs}: {state}", flush=True)

    passed = sum(record["pass"] is True for record in records)
    summary = {
        "schema_version": 1,
        "runs": args.runs,
        "passed": passed,
        "failed": args.runs - passed,
        "server_port": args.server_port,
        "server_jlink": args.server_jlink,
        "client_port": args.client_port,
        "client_jlink": args.client_jlink,
        "records": records,
    }
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"summary: {passed}/{args.runs} pass")
    print(f"artifacts: {args.output}")
    return 0 if passed == args.runs else 1


if __name__ == "__main__":
    raise SystemExit(main())
