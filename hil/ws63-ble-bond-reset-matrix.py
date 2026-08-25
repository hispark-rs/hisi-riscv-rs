#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Run a paired-board WS63 vendor-managed BLE bond reset matrix."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import time
from pathlib import Path

import serial

from jlink_nrst import pulse_nrst


PERIPHERAL_MARKERS = (
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_READY",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_CONNECTED",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_PASSKEY_DISPLAY=[REDACTED]",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_PAIRED",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_AUTH_OK",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_BOND_OBSERVED",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_BOND_OK",
)
PERIPHERAL_RESTORED_MARKERS = (
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_READY",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_CONNECTED",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_PAIRED",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_RESTORED_ACTIVE",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_BOND_OK",
)
CENTRAL_MARKERS = (
    b"RFDBG_RADIO_U5_BLE_SCAN_MATCH",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_CONNECTED",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_PASSKEY_INPUT",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_PASSKEY_ACCEPTED",
    b"RFDBG_RADIO_U5_BLE_PAIR_ACCEPTED",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_PAIRED",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_AUTH_OK",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_BOND_OBSERVED",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_BOND_OK",
)
CENTRAL_RESTORED_MARKERS = (
    b"RFDBG_RADIO_U5_BLE_SCAN_MATCH",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_CONNECTED",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_PAIRED",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_RESTORED_ACTIVE",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_BOND_OK",
)
PERIPHERAL_REMOVAL_MARKERS = (
    *PERIPHERAL_RESTORED_MARKERS,
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_BOND_REMOVED",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_BOND_REMOVE_OK",
)
CENTRAL_REMOVAL_MARKERS = (
    *CENTRAL_RESTORED_MARKERS,
    b"RFDBG_RADIO_U5_BLE_CENTRAL_BOND_REMOVED",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_BOND_REMOVE_OK",
)
PERIPHERAL_REJECT_MARKERS = (
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_READY",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_CONNECTED",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_NEGATIVE_DISCONNECTED",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_REJECT_OK",
)
CENTRAL_REJECT_MARKERS = (
    b"RFDBG_RADIO_U5_BLE_SCAN_MATCH",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_CONNECTED",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_PASSKEY_INPUT",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_REJECT_ACCEPTED",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_NEGATIVE_DISCONNECTED",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_REJECT_OK",
)
PERIPHERAL_STALE_MARKERS = (
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_READY",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_CONNECTED",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_NEGATIVE_DISCONNECTED",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_STALE_OK",
)
CENTRAL_STALE_MARKERS = (
    b"RFDBG_RADIO_U5_BLE_SCAN_MATCH",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_CONNECTED",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_PASSKEY_INPUT",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_STALE_REJECTED",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_NEGATIVE_DISCONNECTED",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_STALE_OK",
)
PERIPHERAL_STARTUP_MARKERS = (
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_BOND_EMPTY",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_BOND_RESTORED",
)
CENTRAL_STARTUP_MARKERS = (
    b"RFDBG_RADIO_U5_BLE_CENTRAL_BOND_EMPTY",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_BOND_RESTORED",
)
FAILURE_MARKERS = (
    b"RFDBG_RADIO_U5_BLE_EVENT_DROP",
    b"RFDBG_RADIO_U5_BLE_BOND_CONSERVATION_ERR",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_ERR",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_ERR",
    b"RFDBG_RADIO_U5_BLE_COMMAND_ERR",
    b"RFDBG_RADIO_U5_BLE_SCAN_LIFECYCLE_ERR",
    b"RFDBG_RADIO_U5_BLE_SCAN_STOP_ERR",
    b"RFDBG_RADIO_U5_BLE_PERIPHERAL_BOND_RESTORE_ERR",
    b"RFDBG_RADIO_U5_BLE_CENTRAL_BOND_RESTORE_ERR",
    b"RFDBG_RADIO_U5_BLE_RESTORED_STATE_ERR",
    b"RFDBG_RADIO_U5_BLE_STATE_QUEUE_ERR",
    b"RFDBG_RADIO_U5_BLE_PASSKEY_QUEUE_ERR",
    b"RFDBG_RADIO_U5_BLE_PASSKEY_RESPONSE_ERR",
    b"RFDBG_RADIO_U5_BLE_NEGATIVE_REQUIRES_EMPTY",
    b"RFDBG_RADIO_U5_BLE_NEGATIVE_PAIRED_ERR",
    b"RFDBG_RADIO_U5_BLE_NEGATIVE_AUTH_ERR",
    b"RFDBG_RADIO_U5_BLE_NEGATIVE_BOND_ERR",
    b"RFDBG_RADIO_U5_BLE_STALE_DISCONNECT_ERR",
    b"RFDBG_RADIO_U5_BLE_NEGATIVE_CONSERVATION_ERR",
    b"RFDBG_NV_WRITE_ERR",
    b"panicked at",
)

PASSKEY_PATTERN = re.compile(
    rb"RFDBG_RADIO_U5_BLE_PERIPHERAL_PASSKEY_DISPLAY=([0-9]{6})"
)
PASSKEY_REDACTED = b"RFDBG_RADIO_U5_BLE_PERIPHERAL_PASSKEY_DISPLAY=[REDACTED]"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def drain(port: serial.Serial) -> bytes:
    waiting = port.in_waiting
    return port.read(waiting) if waiting else b""


def redact_passkeys(data: bytes) -> bytes:
    return PASSKEY_PATTERN.sub(PASSKEY_REDACTED, data)


def marker_contract(
    pairing_mode: str,
    *,
    peripheral: bool,
    restored: bool,
    expect_removal: bool,
) -> tuple[bytes, ...]:
    if pairing_mode == "reject":
        return PERIPHERAL_REJECT_MARKERS if peripheral else CENTRAL_REJECT_MARKERS
    if pairing_mode == "stale":
        return PERIPHERAL_STALE_MARKERS if peripheral else CENTRAL_STALE_MARKERS
    if peripheral:
        if restored and expect_removal:
            return PERIPHERAL_REMOVAL_MARKERS
        return PERIPHERAL_RESTORED_MARKERS if restored else PERIPHERAL_MARKERS
    if restored and expect_removal:
        return CENTRAL_REMOVAL_MARKERS
    return CENTRAL_RESTORED_MARKERS if restored else CENTRAL_MARKERS


def classify(
    peripheral: bytes,
    central: bytes,
    *,
    pairing_mode: str = "passkey",
    expect_removal: bool = False,
) -> dict[str, object]:
    peripheral_restored = PERIPHERAL_STARTUP_MARKERS[1] in peripheral
    central_restored = CENTRAL_STARTUP_MARKERS[1] in central
    peripheral_contract = marker_contract(
        pairing_mode,
        peripheral=True,
        restored=peripheral_restored,
        expect_removal=expect_removal,
    )
    central_contract = marker_contract(
        pairing_mode,
        peripheral=False,
        restored=central_restored,
        expect_removal=expect_removal,
    )
    peripheral_missing = [
        marker.decode() for marker in peripheral_contract if marker not in peripheral
    ]
    central_missing = [
        marker.decode() for marker in central_contract if marker not in central
    ]
    failures = [
        marker.decode()
        for marker in FAILURE_MARKERS
        if marker in peripheral or marker in central
    ]
    peripheral_startup = [
        marker.decode() for marker in PERIPHERAL_STARTUP_MARKERS if marker in peripheral
    ]
    central_startup = [
        marker.decode() for marker in CENTRAL_STARTUP_MARKERS if marker in central
    ]
    role_mismatch = (
        any(marker in peripheral for marker in CENTRAL_STARTUP_MARKERS)
        or any(marker in central for marker in PERIPHERAL_STARTUP_MARKERS)
    )
    startup_valid = len(peripheral_startup) == 1 and len(central_startup) == 1
    restore_mismatch = peripheral_restored != central_restored
    negative_requires_empty = pairing_mode != "passkey" and (
        peripheral_restored or central_restored
    )
    return {
        "pass": not peripheral_missing
        and not central_missing
        and not failures
        and not role_mismatch
        and startup_valid
        and not restore_mismatch
        and not negative_requires_empty,
        "peripheral_missing": peripheral_missing,
        "central_missing": central_missing,
        "failure_markers": failures,
        "peripheral_startup": peripheral_startup,
        "central_startup": central_startup,
        "role_mismatch": role_mismatch,
        "restore_mismatch": restore_mismatch,
        "negative_requires_empty": negative_requires_empty,
        "restored_contract": peripheral_restored and central_restored,
        "peripheral_bytes": len(peripheral),
        "central_bytes": len(central),
    }


def validate_persistence(
    records: list[dict[str, object]],
    *,
    pairing_mode: str = "passkey",
    expect_removal: bool = False,
) -> dict[str, object]:
    """Validate cross-reset persistence, not just each run in isolation."""
    errors: list[str] = []
    if not records:
        errors.append("no reset records")
    else:
        for record in records:
            if record.get("pass") is not True:
                errors.append(
                    f"run {record['run']} did not satisfy the per-run contract"
                )

    if records and pairing_mode != "passkey":
        if len(records) < 2:
            errors.append("a negative pairing mode needs a subsequent reset")
        for record in records:
            if record["restored_contract"]:
                errors.append(
                    f"run {record['run']} restored a bond in {pairing_mode} mode"
                )
    elif records and expect_removal:
        for previous, current in zip(records, records[1:]):
            expected_restored = not previous["restored_contract"]
            if current["restored_contract"] != expected_restored:
                expected = "restored" if expected_restored else "empty"
                errors.append(
                    f"run {current['run']} was not {expected} after run "
                    f"{previous['run']}"
                )
    elif records and not records[0]["restored_contract"]:
        if len(records) < 2:
            errors.append("a fresh pairing needs a subsequent reset to prove restore")
        for record in records[1:]:
            if not record["restored_contract"]:
                errors.append(
                    f"run {record['run']} returned to an empty bond after fresh pairing"
                )
    elif records:
        for record in records[1:]:
            if not record["restored_contract"]:
                errors.append(
                    f"run {record['run']} lost a bond restored by the preceding reset"
                )
    return {
        "proven": not errors,
        "errors": errors,
    }


def empty_capture_roles(record: dict[str, object]) -> list[str]:
    """Return board roles that produced no UART evidence for a reset."""
    return [
        role
        for role in ("peripheral", "central")
        if record[f"{role}_bytes"] == 0
    ]


def capture_run(
    peripheral: serial.Serial,
    central: serial.Serial,
    peripheral_jlink: str,
    central_jlink: str,
    settle: float,
    timeout: float,
    pairing_mode: str,
    expect_removal: bool,
) -> tuple[bytes, bytes]:
    peripheral.reset_input_buffer()
    central.reset_input_buffer()
    pulse_nrst("JLinkExe", peripheral_jlink)
    peripheral_log = bytearray()
    deadline = time.monotonic() + settle
    while time.monotonic() < deadline:
        peripheral_log.extend(drain(peripheral))
        time.sleep(0.01)

    pulse_nrst("JLinkExe", central_jlink)
    central_log = bytearray()
    passkey_relayed = False
    deadline = time.monotonic() + timeout
    complete_at: float | None = None
    while time.monotonic() < deadline:
        peripheral_log.extend(drain(peripheral))
        central_log.extend(drain(central))
        if (
            pairing_mode == "passkey"
            and not passkey_relayed
            and (match := PASSKEY_PATTERN.search(peripheral_log))
        ):
            central.write(b"U5PASS=" + match.group(1) + b"\n")
            central.flush()
            passkey_relayed = True
        bond_complete = (
            b"RFDBG_RADIO_U5_BLE_PERIPHERAL_BOND_OK" in peripheral_log
            and b"RFDBG_RADIO_U5_BLE_CENTRAL_BOND_OK" in central_log
        )
        restored = (
            PERIPHERAL_STARTUP_MARKERS[1] in peripheral_log
            and CENTRAL_STARTUP_MARKERS[1] in central_log
        )
        removal_complete = (
            PERIPHERAL_REMOVAL_MARKERS[-1] in peripheral_log
            and CENTRAL_REMOVAL_MARKERS[-1] in central_log
        )
        negative_complete = {
            "reject": (
                PERIPHERAL_REJECT_MARKERS[-1] in peripheral_log
                and CENTRAL_REJECT_MARKERS[-1] in central_log
            ),
            "stale": (
                PERIPHERAL_STALE_MARKERS[-1] in peripheral_log
                and CENTRAL_STALE_MARKERS[-1] in central_log
            ),
        }.get(pairing_mode, False)
        if pairing_mode == "passkey":
            complete = removal_complete if expect_removal and restored else bond_complete
        else:
            complete = negative_complete
        if complete and complete_at is None:
            complete_at = time.monotonic()
        if complete_at is not None and time.monotonic() - complete_at >= 1.0:
            break
        time.sleep(0.01)
    peripheral_log.extend(drain(peripheral))
    central_log.extend(drain(central))
    return redact_passkeys(bytes(peripheral_log)), redact_passkeys(bytes(central_log))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--peripheral-port", required=True)
    parser.add_argument("--peripheral-jlink", required=True)
    parser.add_argument("--peripheral-elf", type=Path, required=True)
    parser.add_argument("--central-port", required=True)
    parser.add_argument("--central-jlink", required=True)
    parser.add_argument("--central-elf", type=Path, required=True)
    parser.add_argument("--baud", type=int, default=115_200)
    parser.add_argument("--runs", type=int, default=20)
    parser.add_argument("--peripheral-settle", type=float, default=3.0)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument(
        "--pairing-mode",
        choices=("passkey", "reject", "stale"),
        default="passkey",
        help="pairing lifecycle contract implemented by the selected fixture images",
    )
    parser.add_argument(
        "--expect-removal",
        action="store_true",
        help="require restored bonds to be removed and alternate restored/empty boots",
    )
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.runs <= 0 or args.peripheral_settle <= 0 or args.timeout <= 0:
        raise SystemExit("--runs, --peripheral-settle and --timeout must be positive")
    if args.expect_removal and args.pairing_mode != "passkey":
        raise SystemExit("--expect-removal is only valid with --pairing-mode passkey")
    for path in (args.peripheral_elf, args.central_elf):
        if not path.is_file():
            raise SystemExit(f"ELF not found: {path}")
    args.output.mkdir(parents=True, exist_ok=False)

    records: list[dict[str, object]] = []
    abort_reason: str | None = None
    with serial.Serial(args.peripheral_port, args.baud, timeout=0) as peripheral:
        with serial.Serial(args.central_port, args.baud, timeout=0) as central:
            for run in range(1, args.runs + 1):
                peripheral_log, central_log = capture_run(
                    peripheral,
                    central,
                    args.peripheral_jlink,
                    args.central_jlink,
                    args.peripheral_settle,
                    args.timeout,
                    args.pairing_mode,
                    args.expect_removal,
                )
                peripheral_name = f"run-{run:02d}.peripheral.uart.log"
                central_name = f"run-{run:02d}.central.uart.log"
                (args.output / peripheral_name).write_bytes(peripheral_log)
                (args.output / central_name).write_bytes(central_log)
                record = classify(
                    peripheral_log,
                    central_log,
                    pairing_mode=args.pairing_mode,
                    expect_removal=args.expect_removal,
                )
                record.update(
                    {
                        "run": run,
                        "peripheral_log": peripheral_name,
                        "central_log": central_name,
                    }
                )
                records.append(record)
                state = "pass" if record["pass"] else "fail"
                print(f"run {run:02d}/{args.runs}: {state}", flush=True)
                if run == 1 and (empty_roles := empty_capture_roles(record)):
                    abort_reason = (
                        "first reset produced no UART evidence for: "
                        + ", ".join(empty_roles)
                    )
                    print(f"aborting matrix: {abort_reason}", flush=True)
                    break

    passed = sum(record["pass"] is True for record in records)
    persistence = validate_persistence(
        records,
        pairing_mode=args.pairing_mode,
        expect_removal=args.expect_removal,
    )
    executed_runs = len(records)
    contract_pass = (
        executed_runs == args.runs
        and passed == args.runs
        and persistence["proven"]
        and abort_reason is None
    )
    peripheral_restored = sum(
        PERIPHERAL_STARTUP_MARKERS[1].decode() in record["peripheral_startup"]
        for record in records
    )
    central_restored = sum(
        CENTRAL_STARTUP_MARKERS[1].decode() in record["central_startup"]
        for record in records
    )
    summary = {
        "schema_version": 6,
        "pairing_mode": args.pairing_mode,
        "expect_removal": args.expect_removal,
        "runs": args.runs,
        "executed_runs": executed_runs,
        "aborted_early": abort_reason is not None,
        "abort_reason": abort_reason,
        "passed": passed,
        "failed": executed_runs - passed,
        "contract_pass": contract_pass,
        "persistence": persistence,
        "vendor_restore": {
            "peripheral_restored_runs": peripheral_restored,
            "central_restored_runs": central_restored,
        },
        "peripheral": {
            "port": args.peripheral_port,
            "jlink": args.peripheral_jlink,
            "elf": str(args.peripheral_elf),
            "elf_sha256": sha256(args.peripheral_elf),
        },
        "central": {
            "port": args.central_port,
            "jlink": args.central_jlink,
            "elf": str(args.central_elf),
            "elf_sha256": sha256(args.central_elf),
        },
        "records": records,
    }
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"summary: {passed}/{executed_runs} executed pass ({args.runs} planned)")
    print(f"persistence: {'proven' if persistence['proven'] else 'not proven'}")
    print(f"artifacts: {args.output}")
    return 0 if contract_pass else 1


if __name__ == "__main__":
    raise SystemExit(main())
