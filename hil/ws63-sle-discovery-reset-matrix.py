#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Run the paired-board WS63 SLE S1 announce/seek reset matrix."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

import serial

from jlink_nrst import pulse_nrst


ANNOUNCE_MARKERS = (
    b"RFDBG_SLE_S1_INIT_OK",
    b"RFDBG_SLE_S1_ANNOUNCE_OK",
)
SEEK_MARKERS = (
    b"RFDBG_SLE_S1_INIT_OK",
    b"RFDBG_SLE_S1_SEEK_READY",
    b"RFDBG_SLE_S1_SEEK_MATCH",
)
FAILURE_MARKERS = (
    b"RFDBG_MISSING_ROM_CALLBACK",
    b"RFDBG_SLE_S1_INIT_ERR",
    b"RFDBG_SLE_S1_ANNOUNCE_ERR",
    b"RFDBG_SLE_S1_SEEK_ERR",
    b"RFDBG_SLE_S1_EVENT_DROP",
)


def drain(port: serial.Serial) -> bytes:
    waiting = port.in_waiting
    return port.read(waiting) if waiting else b""


def classify(announce: bytes, seek: bytes) -> dict[str, object]:
    announce_missing = [
        marker.decode() for marker in ANNOUNCE_MARKERS if marker not in announce
    ]
    seek_missing = [marker.decode() for marker in SEEK_MARKERS if marker not in seek]
    failures = [
        marker.decode()
        for marker in FAILURE_MARKERS
        if marker in announce or marker in seek
    ]
    return {
        "pass": not announce_missing and not seek_missing and not failures,
        "announce_missing": announce_missing,
        "seek_missing": seek_missing,
        "failure_markers": failures,
        "announce_bytes": len(announce),
        "seek_bytes": len(seek),
    }


def capture_run(
    announce: serial.Serial,
    seek: serial.Serial,
    announce_jlink: str,
    seek_jlink: str,
    announce_settle: float,
    timeout: float,
) -> tuple[bytes, bytes]:
    announce.reset_input_buffer()
    seek.reset_input_buffer()
    pulse_nrst("JLinkExe", announce_jlink)
    announce_log = bytearray()
    settle_deadline = time.monotonic() + announce_settle
    while time.monotonic() < settle_deadline:
        announce_log.extend(drain(announce))
        time.sleep(0.01)

    pulse_nrst("JLinkExe", seek_jlink)
    seek_log = bytearray()
    complete_at: float | None = None
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        announce_log.extend(drain(announce))
        seek_log.extend(drain(seek))
        complete = ANNOUNCE_MARKERS[-1] in announce_log and SEEK_MARKERS[-1] in seek_log
        if complete and complete_at is None:
            complete_at = time.monotonic()
        if complete_at is not None and time.monotonic() - complete_at >= 1.0:
            break
        time.sleep(0.01)

    announce_log.extend(drain(announce))
    seek_log.extend(drain(seek))
    return bytes(announce_log), bytes(seek_log)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--announce-port", required=True)
    parser.add_argument("--announce-jlink", required=True)
    parser.add_argument("--seek-port", required=True)
    parser.add_argument("--seek-jlink", required=True)
    parser.add_argument("--baud", type=int, default=115_200)
    parser.add_argument("--runs", type=int, default=20)
    parser.add_argument("--announce-settle", type=float, default=3.0)
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.runs <= 0 or args.announce_settle <= 0 or args.timeout <= 0:
        raise SystemExit("--runs, --announce-settle and --timeout must be positive")
    args.output.mkdir(parents=True, exist_ok=False)

    records: list[dict[str, object]] = []
    with serial.Serial(args.announce_port, args.baud, timeout=0) as announce:
        with serial.Serial(args.seek_port, args.baud, timeout=0) as seek:
            for run in range(1, args.runs + 1):
                announce_log, seek_log = capture_run(
                    announce,
                    seek,
                    args.announce_jlink,
                    args.seek_jlink,
                    args.announce_settle,
                    args.timeout,
                )
                announce_name = f"run-{run:02d}.announce.uart.log"
                seek_name = f"run-{run:02d}.seek.uart.log"
                (args.output / announce_name).write_bytes(announce_log)
                (args.output / seek_name).write_bytes(seek_log)
                record = classify(announce_log, seek_log)
                record.update(
                    {"run": run, "announce_log": announce_name, "seek_log": seek_name}
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
        "announce_port": args.announce_port,
        "announce_jlink": args.announce_jlink,
        "seek_port": args.seek_port,
        "seek_jlink": args.seek_jlink,
        "records": records,
    }
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"summary: {passed}/{args.runs} pass")
    print(f"artifacts: {args.output}")
    return 0 if passed == args.runs else 1


if __name__ == "__main__":
    raise SystemExit(main())
