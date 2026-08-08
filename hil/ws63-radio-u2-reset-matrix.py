#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Run paired-board WS63 hisi-rf U2/U3 BLE or SLE reset matrices."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

import serial

from jlink_nrst import pulse_nrst


PROTOCOLS = {
    "ble": {
        "source_marker": b"RFDBG_RADIO_U2_BLE_ADV_OK",
        "observer_marker": b"RFDBG_RADIO_U2_BLE_SCAN_OK",
        "failure_prefixes": (b"RFDBG_RADIO_U2_BLE_",),
    },
    "sle": {
        "source_marker": b"RFDBG_RADIO_U2_SLE_ANNOUNCE_OK",
        "observer_marker": b"RFDBG_RADIO_U2_SLE_SEEK_OK",
        "failure_prefixes": (b"RFDBG_RADIO_U2_SLE_",),
    },
    "u3-ble": {
        "source_marker": b"RFDBG_RADIO_U3_BLE_GATT_OK",
        "observer_marker": b"RFDBG_RADIO_U2_BLE_SCAN_OK",
        "failure_prefixes": (b"RFDBG_RADIO_U3_BLE_", b"RFDBG_RADIO_U2_BLE_"),
    },
    "u3-sle": {
        "source_marker": b"RFDBG_RADIO_U3_SLE_SSAP_OK",
        "observer_marker": b"RFDBG_RADIO_U2_SLE_SEEK_OK",
        "failure_prefixes": (b"RFDBG_RADIO_U3_SLE_", b"RFDBG_RADIO_U2_SLE_"),
    },
}
COMMON_MARKER = b"RFDBG_RADIO_U2_INIT_OK"
FAILURE_SUFFIXES = (
    b"INIT_ERR",
    b"QUEUE_ERR",
    b"CORRELATION_ERR",
    b"COMMAND_ERR",
    b"RUNNER_ERR",
    b"LIFECYCLE_ERR",
    b"EVENT_DROP",
)
COMMON_FAILURE_MARKERS = (
    b"RFDBG_MISSING_ROM_CALLBACK",
    b"panicked at",
)


def drain(port: serial.Serial) -> bytes:
    waiting = port.in_waiting
    return port.read(waiting) if waiting else b""


def failure_markers(protocol: str) -> tuple[bytes, ...]:
    prefixes = PROTOCOLS[protocol]["failure_prefixes"]
    assert isinstance(prefixes, tuple)
    return (
        *COMMON_FAILURE_MARKERS,
        *(prefix + suffix for prefix in prefixes for suffix in FAILURE_SUFFIXES),
    )


def classify(protocol: str, source: bytes, observer: bytes) -> dict[str, object]:
    contract = PROTOCOLS[protocol]
    source_marker = contract["source_marker"]
    observer_marker = contract["observer_marker"]
    assert isinstance(source_marker, bytes)
    assert isinstance(observer_marker, bytes)
    required_source = (COMMON_MARKER, source_marker)
    required_observer = (COMMON_MARKER, observer_marker)
    source_missing = [
        marker.decode() for marker in required_source if marker not in source
    ]
    observer_missing = [
        marker.decode() for marker in required_observer if marker not in observer
    ]
    failures = [
        marker.decode()
        for marker in failure_markers(protocol)
        if marker in source or marker in observer
    ]
    return {
        "pass": not source_missing and not observer_missing and not failures,
        "source_missing": source_missing,
        "observer_missing": observer_missing,
        "failure_markers": failures,
        "source_bytes": len(source),
        "observer_bytes": len(observer),
    }


def capture_run(
    source: serial.Serial,
    observer: serial.Serial,
    source_jlink: str,
    observer_jlink: str,
    source_settle: float,
    timeout: float,
    observer_marker: bytes,
) -> tuple[bytes, bytes]:
    source.reset_input_buffer()
    observer.reset_input_buffer()
    pulse_nrst("JLinkExe", source_jlink)
    source_log = bytearray()
    settle_deadline = time.monotonic() + source_settle
    while time.monotonic() < settle_deadline:
        source_log.extend(drain(source))
        time.sleep(0.01)

    pulse_nrst("JLinkExe", observer_jlink)
    observer_log = bytearray()
    complete_at: float | None = None
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        source_log.extend(drain(source))
        observer_log.extend(drain(observer))
        if observer_marker in observer_log and complete_at is None:
            complete_at = time.monotonic()
        if complete_at is not None and time.monotonic() - complete_at >= 1.0:
            break
        time.sleep(0.01)

    source_log.extend(drain(source))
    observer_log.extend(drain(observer))
    return bytes(source_log), bytes(observer_log)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--protocol", choices=tuple(PROTOCOLS), required=True)
    parser.add_argument("--source-port", required=True)
    parser.add_argument("--source-jlink", required=True)
    parser.add_argument("--observer-port", required=True)
    parser.add_argument("--observer-jlink", required=True)
    parser.add_argument("--baud", type=int, default=115_200)
    parser.add_argument("--runs", type=int, default=20)
    parser.add_argument("--source-settle", type=float, default=3.0)
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.runs <= 0 or args.source_settle <= 0 or args.timeout <= 0:
        raise SystemExit("--runs, --source-settle and --timeout must be positive")
    args.output.mkdir(parents=True, exist_ok=False)

    observer_marker = PROTOCOLS[args.protocol]["observer_marker"]
    assert isinstance(observer_marker, bytes)
    records: list[dict[str, object]] = []
    with serial.Serial(args.source_port, args.baud, timeout=0) as source:
        with serial.Serial(args.observer_port, args.baud, timeout=0) as observer:
            for run in range(1, args.runs + 1):
                source_log, observer_log = capture_run(
                    source,
                    observer,
                    args.source_jlink,
                    args.observer_jlink,
                    args.source_settle,
                    args.timeout,
                    observer_marker,
                )
                source_name = f"run-{run:02d}.source.uart.log"
                observer_name = f"run-{run:02d}.observer.uart.log"
                (args.output / source_name).write_bytes(source_log)
                (args.output / observer_name).write_bytes(observer_log)
                record = classify(args.protocol, source_log, observer_log)
                record.update(
                    {
                        "run": run,
                        "source_log": source_name,
                        "observer_log": observer_name,
                    }
                )
                records.append(record)
                state = "pass" if record["pass"] else "fail"
                print(f"run {run:02d}/{args.runs}: {state}", flush=True)

    passed = sum(record["pass"] is True for record in records)
    summary = {
        "schema_version": 1,
        "protocol": args.protocol,
        "runs": args.runs,
        "passed": passed,
        "failed": args.runs - passed,
        "source_port": args.source_port,
        "source_jlink": args.source_jlink,
        "observer_port": args.observer_port,
        "observer_jlink": args.observer_jlink,
        "records": records,
    }
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"summary: {passed}/{args.runs} pass")
    print(f"artifacts: {args.output}")
    return 0 if passed == args.runs else 1


if __name__ == "__main__":
    raise SystemExit(main())
