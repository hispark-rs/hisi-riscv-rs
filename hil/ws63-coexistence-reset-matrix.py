#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyserial"]
# ///
"""Run the two-board WS63 Wi-Fi/BLE and Wi-Fi/SLE init matrix."""

from __future__ import annotations

import argparse
import hashlib
import json
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
}
FAILURE_MARKERS = (
    b"panicked at",
    b"RFDBG_MISSING_ROM_CALLBACK",
    b"RFDBG_COEX_INIT_ERR",
    b"scheduler contract violation",
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


def classify(role: str, payload: bytes) -> dict[str, object]:
    required = COMMON_MARKERS + ROLE_MARKERS[role]
    missing = [marker.decode() for marker in required if marker not in payload]
    failures = [marker.decode() for marker in FAILURE_MARKERS if marker in payload]
    return {
        "pass": not missing and not failures,
        "missing": missing,
        "failure_markers": failures,
        "bytes": len(payload),
    }


def capture_pair(
    ble: serial.Serial,
    sle: serial.Serial,
    ble_jlink: str,
    sle_jlink: str,
    timeout: float,
) -> tuple[bytes, bytes]:
    ble.reset_input_buffer()
    sle.reset_input_buffer()
    pulse_nrst("JLinkExe", ble_jlink)
    pulse_nrst("JLinkExe", sle_jlink)

    ble_log = bytearray()
    sle_log = bytearray()
    complete_at: float | None = None
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        ble_log.extend(drain(ble))
        sle_log.extend(drain(sle))
        both_complete = (
            COMMON_MARKERS[-1] in ble_log and COMMON_MARKERS[-1] in sle_log
        )
        if both_complete and complete_at is None:
            complete_at = time.monotonic()
        if complete_at is not None and time.monotonic() - complete_at >= 0.5:
            break
        time.sleep(0.01)

    ble_log.extend(drain(ble))
    sle_log.extend(drain(sle))
    return bytes(ble_log), bytes(sle_log)


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
            for run in range(1, args.runs + 1):
                ble_log, sle_log = capture_pair(
                    ble, sle, args.ble_jlink, args.sle_jlink, args.timeout
                )
                ble_name = f"run-{run:02d}.ble.uart.log"
                sle_name = f"run-{run:02d}.sle.uart.log"
                (args.output / ble_name).write_bytes(ble_log)
                (args.output / sle_name).write_bytes(sle_log)
                ble_result = classify("ble", ble_log)
                sle_result = classify("sle", sle_log)
                passed = ble_result["pass"] is True and sle_result["pass"] is True
                records.append(
                    {
                        "run": run,
                        "pass": passed,
                        "ble": {**ble_result, "log": ble_name},
                        "sle": {**sle_result, "log": sle_name},
                    }
                )
                print(
                    f"run {run:02d}/{args.runs}: {'pass' if passed else 'fail'}",
                    flush=True,
                )

    passed = sum(record["pass"] is True for record in records)
    summary = {
        "schema_version": 1,
        "contract": "ws63-wifi-bgle-shared-init/v1",
        "runs": args.runs,
        "passed": passed,
        "failed": args.runs - passed,
        "ble": {
            "port": args.ble_port,
            "jlink": args.ble_jlink,
            "elf": str(args.ble_elf),
            "elf_sha256": sha256(args.ble_elf),
        },
        "sle": {
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
