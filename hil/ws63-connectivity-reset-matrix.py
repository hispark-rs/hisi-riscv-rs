#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Measure WS63 connectivity reliability across unchanged-image nRST boots."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

import serial


RUST_TERMINAL_MARKERS = (
    b"RF5C_PING_OK",
    b"RF5C_PING_TIMEOUT",
    b"RF5B_WPA_CONNECT_ERR:",
    b"RF5B_CONNECT_ERR:",
    b"RF5B_AP_NOT_FOUND",
    b"RF3_SCAN_ERR:",
    b"RF2_INIT_ERR:",
    b"RFDBG_EXCEPTION",
)

RUST_TIMED_MARKERS = (
    b"RF1_IMAGE_OK",
    b"RF2_INIT_OK",
    b"RF3_SCAN_OK",
    b"RF5B_CONNECT_BEGIN",
    b"RF5B_WPA_CONNECT_OK",
    b"RF5A_DHCP_OK",
    b"RF5A_ARP_OK",
    b"RF5C_PING_BEGIN",
    b"RF5C_PING_OK",
)

# fbb_ws63 log_def_wifi.h maps file 0x13 to hmac_sme_sta.c; line 0x44f
# reports WLAN_AUTH_RSP2_TIMEOUT (5201) and the vendor retry count.
AUTH_RSP2_TIMEOUT_EVENT = b"file=0x00000013 line=0x0000044f"
OFFICIAL_AUTH_RSP2_TIMEOUT_EVENT = b"wifi:auth fail[5201]"

OFFICIAL_TERMINAL_MARKERS = (b"APP|[WIFI_STA_SAMPLE]::STA DHCP success.",)
OFFICIAL_TIMED_MARKERS = (
    b"APP|[WIFI_STA_SAMPLE]::wifi init succ.",
    b"APP|[WIFI_STA_SAMPLE]::Scan start!",
    b"APP|[WIFI_STA_SAMPLE]::Scan done!.",
    b"APP|[WIFI_STA_SAMPLE]::Connect start.",
    b"wifi:rx auth seq 2 alg 0 code 0",
    b"+NOTICE:CONNECTED",
    b"APP|[WIFI_STA_SAMPLE]::STA DHCP success.",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", default=os.environ.get("PORT"), required="PORT" not in os.environ)
    parser.add_argument("--baud", type=int, default=115_200)
    parser.add_argument("--runs", type=int, default=20)
    parser.add_argument("--timeout", type=float, default=65.0)
    parser.add_argument("--profile", choices=("rust", "official-liteos"), default="rust")
    parser.add_argument(
        "--stage",
        choices=("connect", "connectivity"),
        default="connectivity",
        help="stop after association or continue through the IP connectivity probe",
    )
    parser.add_argument("--jlink", default="JLinkExe")
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def pulse_nrst(jlink: str) -> None:
    with tempfile.NamedTemporaryFile("w", suffix=".jlink", delete=False) as command_file:
        command_file.write("SetRESET\n")
        command_file.write("sleep 200\n")
        command_file.write("ClrRESET\n")
        command_file.write("sleep 100\n")
        command_file.write("q\n")
        command_path = Path(command_file.name)
    try:
        result = subprocess.run(
            [jlink, "-NoGui", "1", "-CommandFile", str(command_path)],
            capture_output=True,
            timeout=10,
            check=False,
        )
        if result.returncode != 0:
            detail = (result.stderr or result.stdout).decode(errors="replace").strip()
            raise RuntimeError(f"J-Link nRST failed with {result.returncode}: {detail}")
    finally:
        command_path.unlink(missing_ok=True)


def classify(log: bytes, profile: str, stage: str) -> str:
    if profile == "official-liteos":
        if stage == "connect" and b"+NOTICE:CONNECTED" in log:
            return "pass"
        if b"APP|[WIFI_STA_SAMPLE]::STA DHCP success." in log:
            return "pass"
        if OFFICIAL_AUTH_RSP2_TIMEOUT_EVENT in log:
            return "auth_rsp2_timeout"
        if b"APP|[WIFI_STA_SAMPLE]::Connect fail!." in log:
            return "connect_error"
        return "capture_timeout"

    if stage == "connect" and b"RF5B_WPA_CONNECT_OK" in log:
        return "pass"
    if b"RF5C_PING_OK" in log:
        return "pass"
    if b"RF5C_PING_TIMEOUT" in log:
        return "ping_timeout"
    if b"RF5B_WPA_CONNECT_ERR:0x00001451" in log:
        return "auth_rsp2_timeout"
    if b"RF5B_WPA_CONNECT_ERR:" in log or b"RF5B_CONNECT_ERR:" in log:
        return "connect_error"
    if b"RF5B_AP_NOT_FOUND" in log:
        return "ap_not_found"
    if b"RF3_SCAN_ERR:" in log:
        return "scan_error"
    if b"RF2_INIT_ERR:" in log:
        return "init_error"
    if b"RFDBG_EXCEPTION" in log:
        return "exception"
    return "capture_timeout"


def capture_run(
    port: serial.Serial, jlink: str, timeout: float, profile: str, stage: str
) -> tuple[bytes, dict[str, float]]:
    port.reset_input_buffer()
    started = time.monotonic()
    pulse_nrst(jlink)
    log = bytearray()
    marker_times: dict[str, float] = {}
    deadline = started + timeout

    timed_markers = OFFICIAL_TIMED_MARKERS if profile == "official-liteos" else RUST_TIMED_MARKERS
    terminal_markers = (
        OFFICIAL_TERMINAL_MARKERS if profile == "official-liteos" else RUST_TERMINAL_MARKERS
    )
    if stage == "connect":
        success_marker = (
            b"+NOTICE:CONNECTED"
            if profile == "official-liteos"
            else b"RF5B_WPA_CONNECT_OK"
        )
        terminal_markers = (*terminal_markers, success_marker)

    while time.monotonic() < deadline:
        chunk = port.read(4096)
        if not chunk:
            continue
        log.extend(chunk)
        now = time.monotonic() - started
        for marker in timed_markers:
            name = marker.decode()
            if name not in marker_times and marker in log:
                marker_times[name] = round(now, 3)
        if any(marker in log for marker in terminal_markers):
            break

    return bytes(log), marker_times


def main() -> int:
    args = parse_args()
    if args.runs <= 0 or args.timeout <= 0:
        raise SystemExit("--runs and --timeout must be positive")

    timestamp = time.strftime("%Y%m%d-%H%M%S")
    output = args.output or Path("/private/tmp") / f"ws63-connectivity-reset-matrix-{timestamp}"
    output.mkdir(parents=True, exist_ok=False)
    records: list[dict[str, object]] = []

    # Open UART before every reset pulse so boot and early failure markers are
    # observable. Keep the descriptor open across runs to avoid driver churn.
    with serial.Serial(args.port, args.baud, timeout=0.2) as port:
        for run in range(1, args.runs + 1):
            log, marker_times = capture_run(
                port, args.jlink, args.timeout, args.profile, args.stage
            )
            result = classify(log, args.profile, args.stage)
            log_path = output / f"run-{run:02d}.uart.log"
            log_path.write_bytes(log)
            record = {
                "run": run,
                "result": result,
                "bytes": len(log),
                "auth_rsp2_timeouts": log.count(AUTH_RSP2_TIMEOUT_EVENT)
                + log.count(OFFICIAL_AUTH_RSP2_TIMEOUT_EVENT),
                "marker_seconds": marker_times,
                "log": log_path.name,
            }
            records.append(record)
            print(
                f"run {run:02d}/{args.runs}: {result} "
                f"(auth_rsp2_timeouts={record['auth_rsp2_timeouts']}, {len(log)} bytes)",
                flush=True,
            )

    counts: dict[str, int] = {}
    for record in records:
        result = str(record["result"])
        counts[result] = counts.get(result, 0) + 1
    summary = {
        "port": args.port,
        "baud": args.baud,
        "profile": args.profile,
        "stage": args.stage,
        "runs": args.runs,
        "timeout_seconds": args.timeout,
        "counts": counts,
        "auth_rsp2_timeouts": sum(int(record["auth_rsp2_timeouts"]) for record in records),
        "records": records,
    }
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"summary: {json.dumps(counts, sort_keys=True)}")
    print(f"artifacts: {output}")
    return 0 if counts.get("pass", 0) == args.runs else 1


if __name__ == "__main__":
    raise SystemExit(main())
