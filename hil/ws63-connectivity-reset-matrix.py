#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Measure WS63 connectivity reliability across unchanged-image nRST boots."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

import serial


RUST_TERMINAL_MARKERS = (
    b"RF5C_CONNECTIVITY_SUMMARY",
    b"RFDBG_A5B_CONNECT_PROFILE_OK",
    b"RFDBG_A5B_CONNECT_ERR",
    b"RFDBG_A5B_SCAN_ERR",
    b"RF5B_WPA_CONNECT_ERR:",
    b"RF5B_CONNECT_ERR:",
    b"W2D_WPA2_CONNECT_ERR",
    b"W2E_WPA3_CONNECT_ERR",
    b"A4_NET_ERR",
    b"RF5B_AP_NOT_FOUND",
    b"RF3_SCAN_ERR",
    b"RF2_INIT_ERR:",
    b"RFDBG_EXCEPTION",
)

RUST_TIMED_MARKERS = (
    b"RFDBG_A5B_SCAN_OK",
    b"RFDBG_A5B_CONNECT_OK",
    b"RFDBG_A5B_CONNECT_PROFILE_OK",
    b"RF1_IMAGE_OK",
    b"RF2_INIT_OK",
    b"RF3_SCAN_OK",
    b"RF5B_CONNECT_BEGIN",
    b"RF5B_WPA_CONNECT_OK",
    b"W2D_WPA2_CONNECT_OK",
    b"W2E_WPA3_CONNECT_OK",
    b"RF5A_DHCP_OK",
    b"RF5A_ARP_OK",
    b"RF5C_PING_SERIES_BEGIN",
    b"RF5C_PING_OK",
    b"RF5C_CONNECTIVITY_SUMMARY",
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

A5B_METRIC_MARKERS = {
    "event_queue": (
        b"RFDBG_A5B_EVENT ",
        ("pending", "high_water", "dropped"),
    ),
    "control_queue": (
        b"RFDBG_A5B_CONTROL ",
        ("pending", "high_water"),
    ),
    "runner": (
        b"RFDBG_A5B_RUNNER run=",
        (
            "run",
            "waits",
            "wake",
            "immediate",
            "operations",
            "completed",
            "pending",
            "exhausted",
            "errors",
        ),
    ),
    "wait": (
        b"RFDBG_A5B_WAIT ",
        ("backend", "l2", "waker", "polls", "pending", "ready", "timer"),
    ),
    "blocking": (
        b"RFDBG_A5B_BLOCKING ",
        (
            "init_calls",
            "init_max_ms",
            "scan_calls",
            "poll_calls",
            "internal_sleep",
            "supplicant_poll",
        ),
    ),
}

ARTIFACT_IDENTITY_SCHEMA = "hisi-connectivity-artifact/v1"
MARKER_CONTRACT = "ws63-connectivity-markers/v1"

RUST_FATAL_MARKERS = (
    b"A4_NET_ERR",
    b"RF2_INIT_ERR:",
    b"RF3_SCAN_ERR",
    b"RF5B_AP_NOT_FOUND",
    b"RF5B_CONNECT_ERR:",
    b"RF5B_WPA_CONNECT_ERR:",
    b"W2D_WPA2_CONNECT_ERR",
    b"W2E_WPA3_CONNECT_ERR",
    b"RFDBG_A5B_SCAN_ERR",
    b"RFDBG_A5B_CONNECT_ERR",
    b"RFDBG_A5B_RUNNER_ERR",
    b"RFDBG_A5B_WAIT_ERR",
    b"RFDBG_EXCEPTION",
    b"panicked at",
    b"scheduler contract violation",
    b"BUDGET_VIOLATION",
)

QEMU_CONTRACT_FIXTURE_MARKER = b"RFDBG_CONNECTIVITY_CONTRACT_FIXTURE"
PUBLIC_ICMP_OBSERVATION_TARGET = "223.5.5.5"
L2_PROTOCOL_FIELDS = (
    "rx_arp_req",
    "rx_arp_reply",
    "rx_ipv4",
    "rx_other",
    "tx_arp_req",
    "tx_arp_reply",
    "tx_ipv4",
    "tx_other",
)

RUST_STAGE_MARKERS = {
    "init-scan": (
        ("image", (b"RF1_IMAGE_OK",)),
        ("initialize", (b"RF2_INIT_OK",)),
        ("initialized_event", (b"A4_RADIO_EVENT kind=initialized",)),
        ("scan", (b"RF3_SCAN_OK",)),
        ("scan_event", (b"A4_RADIO_EVENT kind=scan-completed",)),
    ),
    "connectivity": (
        ("image", (b"RF1_IMAGE_OK",)),
        ("initialize", (b"RF2_INIT_OK",)),
        ("initialized_event", (b"A4_RADIO_EVENT kind=initialized",)),
        ("scan", (b"RF3_SCAN_OK",)),
        ("scan_event", (b"A4_RADIO_EVENT kind=scan-completed",)),
        (
            "connect",
            (
                b"RF5B_WPA_CONNECT_OK",
                b"W2D_WPA2_CONNECT_OK",
                b"W2E_WPA3_CONNECT_OK",
            ),
        ),
        ("connected_event", (b"A4_RADIO_EVENT kind=connected",)),
        ("dhcp", (b"RF5A_DHCP_OK",)),
        ("neighbor", (b"RF5A_ARP_OK",)),
        ("local_data_path", (b"RF5C_LOCAL_DATA_PATH_OK",)),
        ("summary", (b"RF5C_CONNECTIVITY_SUMMARY",)),
        ("steady", (b"A4_NET_RUNNER_STEADY",)),
        ("dhcp_renew", (b"A4_DHCP_RENEW_OK",)),
    ),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", default=os.environ.get("PORT"))
    parser.add_argument(
        "--analyze-dir",
        type=Path,
        help="reclassify existing run-*.uart.log files without resetting hardware",
    )
    parser.add_argument("--baud", type=int, default=115_200)
    parser.add_argument("--runs", type=int, default=20)
    parser.add_argument("--timeout", type=float, default=65.0)
    parser.add_argument(
        "--post-terminal-seconds",
        type=float,
        default=1.0,
        help="continue capturing briefly after the terminal marker for trailing diagnostics",
    )
    parser.add_argument("--profile", choices=("rust", "official-liteos"), default="rust")
    parser.add_argument(
        "--stage",
        choices=("init-scan", "connect", "connectivity"),
        default="connectivity",
        help="stop after association or continue through the IP connectivity probe",
    )
    parser.add_argument(
        "--required-ap-mode",
        choices=("pure-wpa3", "transition"),
        help="require every Rust run to report the selected WPA3 RSNE mode",
    )
    parser.add_argument("--jlink", default="JLinkExe")
    parser.add_argument(
        "--reference-target",
        help="optional same-network target to ping once from the host (for example the AP gateway)",
    )
    parser.add_argument("--reference-count", type=int, default=5)
    parser.add_argument("--output", type=Path)
    parser.add_argument(
        "--require-contract",
        action="store_true",
        help="fail closed on missing/out-of-order stage markers or non-zero diagnostics",
    )
    parser.add_argument(
        "--qemu-contract-fixture",
        action="store_true",
        help="require the explicit contract-only QEMU fixture marker",
    )
    parser.add_argument(
        "--max-runner-step-ms",
        type=int,
        help="maximum accepted A5B runner turn when --require-contract is set",
    )
    parser.add_argument(
        "--write-artifact-identity",
        type=Path,
        help="write a deterministic ELF/profile identity manifest and exit",
    )
    parser.add_argument(
        "--artifact-identity",
        type=Path,
        help="verify this identity manifest before classifying captures",
    )
    parser.add_argument("--elf", type=Path, help="ELF covered by the identity manifest")
    parser.add_argument(
        "--profile-id",
        help="build profile covered by the identity manifest",
    )
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


def parse_ping_summaries(log: bytes) -> dict[str, dict[str, int | str]]:
    summaries: dict[str, dict[str, int | str]] = {}
    for line in log.splitlines():
        if not line.startswith((b"RF5C_PING_OK ", b"RF5C_PING_TIMEOUT ")):
            continue
        fields: dict[str, int | str] = {
            "status": "ok" if line.startswith(b"RF5C_PING_OK ") else "timeout"
        }
        for token in line.decode(errors="replace").split()[1:]:
            if "=" not in token:
                continue
            key, value = token.split("=", 1)
            if key == "target":
                fields[key] = value
            elif value.lower().startswith("0x"):
                try:
                    fields[key] = int(value, 16)
                except ValueError:
                    continue
        target = fields.get("target")
        if isinstance(target, str):
            summaries[target] = fields
    return summaries


def run_reference_ping(target: str, count: int, output: Path) -> dict[str, object]:
    result = subprocess.run(
        ["ping", "-c", str(count), target],
        capture_output=True,
        text=True,
        timeout=max(10, count * 3),
        check=False,
    )
    text = result.stdout + result.stderr
    output.write_text(text)
    loss_match = re.search(r"([0-9.]+)% packet loss", text)
    rtt_match = re.search(
        r"(?:round-trip|rtt)[^=]*=\s*([0-9.]+)/([0-9.]+)/([0-9.]+)", text
    )
    return {
        "target": target,
        "count": count,
        "exit_code": result.returncode,
        "packet_loss_percent": float(loss_match.group(1)) if loss_match else None,
        "rtt_min_ms": float(rtt_match.group(1)) if rtt_match else None,
        "rtt_avg_ms": float(rtt_match.group(2)) if rtt_match else None,
        "rtt_max_ms": float(rtt_match.group(3)) if rtt_match else None,
        "log": output.name,
    }


def detected_ap_mode(log: bytes) -> str | None:
    for mode in ("pure-wpa3", "transition"):
        if f"W2E_AP_SECURITY mode={mode}".encode() in log:
            return mode
    return None


def parse_hex_fields(line: bytes) -> dict[str, int]:
    fields: dict[str, int] = {}
    for token in line.decode(errors="replace").split()[1:]:
        if "=" not in token:
            continue
        key, value = token.split("=", 1)
        if not value.lower().startswith("0x"):
            continue
        try:
            fields[key] = int(value, 16)
        except ValueError:
            continue
    return fields


def parse_hex_values(line: bytes) -> list[int]:
    values: list[int] = []
    for token in line.decode(errors="replace").split()[1:]:
        if not token.lower().startswith("0x"):
            continue
        try:
            values.append(int(token, 16))
        except ValueError:
            continue
    return values


def sha256_path(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def artifact_identity(elf: Path, profile_id: str) -> dict[str, str]:
    return {
        "schema": ARTIFACT_IDENTITY_SCHEMA,
        "marker_contract": MARKER_CONTRACT,
        "profile_id": profile_id,
        "elf_sha256": sha256_path(elf),
    }


def write_artifact_identity(path: Path, elf: Path, profile_id: str) -> dict[str, str]:
    identity = artifact_identity(elf, profile_id)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(identity, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return identity


def verify_artifact_identity(
    path: Path, elf: Path, profile_id: str
) -> dict[str, str]:
    try:
        expected = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read artifact identity: {error}") from error
    if not isinstance(expected, dict):
        raise ValueError("artifact identity must be a JSON object")

    actual = artifact_identity(elf, profile_id)
    mismatches = [
        key
        for key, value in actual.items()
        if expected.get(key) != value
    ]
    unknown = sorted(set(expected) - set(actual))
    if unknown:
        mismatches.extend(f"unknown:{key}" for key in unknown)
    if mismatches:
        raise ValueError(
            "artifact identity mismatch: " + ", ".join(sorted(mismatches))
        )
    actual["manifest_sha256"] = sha256_path(path)
    return actual


def marker_group_position(
    log: bytes, markers: tuple[bytes, ...], after: int
) -> int | None:
    positions = [position for marker in markers if (position := log.find(marker, after)) >= 0]
    return min(positions) if positions else None


def last_prefixed_line(log: bytes, prefix: bytes) -> bytes | None:
    return next(
        (line for line in reversed(log.splitlines()) if line.startswith(prefix)),
        None,
    )


def validate_a5b_metrics(
    log: bytes,
    max_runner_step_ms: int | None,
    *,
    require_disconnect: bool,
) -> list[str]:
    """Validate the bounded incremental-runner trailer for one boot."""
    if b"RFDBG_A5B_CONNECT_PROFILE_OK" not in log:
        return (
            ["missing:a5b_connect_profile"]
            if max_runner_step_ms is not None
            else []
        )

    metrics = parse_a5b_metrics(log, require_disconnect=require_disconnect)
    if metrics is None or metrics.get("complete") is not True:
        missing = [] if metrics is None else metrics.get("missing", [])
        suffix = ",".join(str(value) for value in missing)
        return [f"a5b_metrics_incomplete:{suffix}"]

    violations: list[str] = []
    event = metrics["event_queue"]
    runner = metrics["runner"]
    blocking = metrics["blocking"]
    timings = metrics["timings"]
    assert isinstance(event, dict)
    assert isinstance(runner, dict)
    assert isinstance(blocking, dict)
    assert isinstance(timings, dict)
    if event["dropped"] != 0:
        violations.append("nonzero:event_queue.dropped")
    if runner["errors"] != 0:
        violations.append("nonzero:runner.errors")
    for field in (
        "scan_calls",
        "poll_calls",
        "internal_sleep",
        "supplicant_poll",
    ):
        if blocking[field] != 0:
            violations.append(f"nonzero:blocking.{field}")
    if (
        max_runner_step_ms is not None
        and timings["runner_step_max_ms"] > max_runner_step_ms
    ):
        violations.append(
            "budget:runner_step_max_ms="
            f"{timings['runner_step_max_ms']}>{max_runner_step_ms}"
        )
    return violations


def validate_rust_contract(
    log: bytes,
    stage: str,
    max_runner_step_ms: int | None = None,
    qemu_contract_fixture: bool = False,
) -> list[str]:
    """Return fail-closed marker/diagnostic violations for one Rust boot."""
    violations: list[str] = []

    has_fixture_marker = QEMU_CONTRACT_FIXTURE_MARKER in log
    if qemu_contract_fixture and not has_fixture_marker:
        violations.append("missing:qemu_contract_fixture")
    if not qemu_contract_fixture and has_fixture_marker:
        violations.append("forbidden:qemu_contract_fixture")

    for marker in RUST_FATAL_MARKERS:
        if marker in log:
            violations.append(f"fatal:{marker.decode(errors='replace')}")

    stage_markers = RUST_STAGE_MARKERS.get(stage)
    if stage_markers is not None:
        cursor = 0
        for name, markers in stage_markers:
            position = marker_group_position(log, markers, cursor)
            if position is None:
                violations.append(f"missing:{name}")
                continue
            if position < cursor:
                violations.append(f"out_of_order:{name}")
                continue
            cursor = position + 1

        scan_line = last_prefixed_line(log, b"RF3_SCAN_OK ")
        if scan_line is not None:
            scan = parse_hex_fields(scan_line)
            if scan.get("count", 0) <= 0:
                violations.append("invalid:scan.count")

    if stage in ("connect", "connectivity"):
        violations.extend(
            validate_a5b_metrics(
                log,
                max_runner_step_ms,
                require_disconnect=stage == "connect",
            )
        )

    if stage == "connect":
        if (
            b"RFDBG_A5B_CONNECT_PROFILE_OK" not in log
            and not any(
            marker in log
            for marker in (
                b"RF5B_WPA_CONNECT_OK",
                b"W2D_WPA2_CONNECT_OK",
                b"W2E_WPA3_CONNECT_OK",
            )
            )
        ):
            violations.append("missing:connect")

    if stage == "connectivity":
        if (
            b"W2E_WPA3_CONNECT_OK" in log
            and b"W2E_WPA3_CONNECT_OK pmf=required" not in log
        ):
            violations.append("invalid:wpa3.pmf")
        summary_line = last_prefixed_line(log, b"RF5C_CONNECTIVITY_SUMMARY ")
        if summary_line is not None:
            summary = parse_hex_fields(summary_line)
            for field in (
                "gateway_tx",
                "gateway_rx",
                "public_tx",
                "public_rx",
                "rx_queue_drop",
            ):
                if field not in summary:
                    violations.append(f"missing:summary.{field}")
            if summary.get("gateway_tx", 0) <= 0:
                violations.append("invalid:summary.gateway_tx")
            if summary.get("gateway_rx", 0) <= 0:
                violations.append("invalid:summary.gateway_rx")
            if summary.get("rx_queue_drop", 0) != 0:
                violations.append("nonzero:summary.rx_queue_drop")

        public_ping = parse_ping_summaries(log).get(PUBLIC_ICMP_OBSERVATION_TARGET)
        if public_ping is not None:
            if int(public_ping.get("tx", 0)) <= 0:
                violations.append("invalid:public_ping.tx")
            if int(public_ping.get("tx_error", 0)) != 0:
                violations.append("nonzero:public_ping.tx_error")
            if int(public_ping.get("rx_queue_drop", 0)) != 0:
                violations.append("nonzero:public_ping.rx_queue_drop")

        renew_line = last_prefixed_line(log, b"A4_DHCP_RENEW_OK ")
        if renew_line is not None:
            renew = parse_hex_fields(renew_line)
            if renew.get("client", 0) <= 0:
                violations.append("invalid:dhcp_renew.client")
            if renew.get("server", 0) <= 0:
                violations.append("invalid:dhcp_renew.server")

    return violations


def parse_a5b_metrics(
    log: bytes,
    *,
    require_disconnect: bool = True,
) -> dict[str, object] | None:
    """Parse the bounded A5B diagnostic trailer from one unchanged-image boot."""
    lines = log.splitlines()
    if not any(
        line.startswith(prefix)
        for line in lines
        for prefix, _ in A5B_METRIC_MARKERS.values()
    ):
        return None

    metrics: dict[str, object] = {}
    missing: list[str] = []
    for section, (prefix, required_fields) in A5B_METRIC_MARKERS.items():
        line = next((candidate for candidate in reversed(lines) if candidate.startswith(prefix)), None)
        if line is None:
            missing.append(section)
            continue
        fields = parse_hex_fields(line)
        absent = [field for field in required_fields if field not in fields]
        if absent:
            missing.extend(f"{section}.{field}" for field in absent)
            continue
        metrics[section] = {field: fields[field] for field in required_fields}

    runner_steps = [
        fields["value"]
        for line in lines
        if line.startswith(b"RFDBG_A5B_RUNNER_ELAPSED_MS ")
        and "value" in (fields := parse_hex_fields(line))
    ]
    timings: dict[str, int] = {}
    timing_markers = [
        ("initialize_elapsed_ms", b"RFDBG_A5B_INITIALIZE_OK "),
        ("scan_elapsed_ms", b"RFDBG_A5B_SCAN_OK "),
        ("connect_elapsed_ms", b"RFDBG_A5B_CONNECT_OK "),
    ]
    if require_disconnect:
        timing_markers.append(
            ("disconnect_elapsed_ms", b"RFDBG_A5B_DISCONNECT_OK ")
        )
    for name, prefix in timing_markers:
        line = next((candidate for candidate in reversed(lines) if candidate.startswith(prefix)), None)
        if line is None:
            missing.append(name)
            continue
        fields = parse_hex_fields(line)
        if "elapsed_ms" not in fields:
            missing.append(name)
            continue
        timings[name] = fields["elapsed_ms"]

    association_ioctl = next(
        (
            candidate
            for candidate in reversed(lines)
            if candidate.startswith(b"RFDBG_A5B_CONNECT_ASSOC_IOCTL ")
        ),
        None,
    )
    association_values = (
        parse_hex_values(association_ioctl) if association_ioctl is not None else []
    )
    if len(association_values) == 12:
        timings["association_ioctl_max_ms"] = max(
            association_values[index] for index in (2, 5, 8, 11)
        )
    else:
        missing.append("association_ioctl")

    if runner_steps:
        timings["runner_step_max_ms"] = max(runner_steps)
        timings["runner_step_count"] = len(runner_steps)
    else:
        missing.append("runner_step_elapsed")
    metrics["timings"] = timings
    metrics["complete"] = not missing
    metrics["missing"] = missing
    return metrics


def parse_l2_protocol_diagnostics(log: bytes) -> dict[str, int] | None:
    """Parse the optional bounded Ethernet protocol-class snapshot."""
    line = last_prefixed_line(log, b"RFDBG_A5B_L2 ")
    if line is None:
        return None
    fields = parse_hex_fields(line)
    return {field: fields[field] for field in L2_PROTOCOL_FIELDS if field in fields}


def aggregate_a5b_metrics(records: list[dict[str, object]]) -> dict[str, object] | None:
    parsed = [
        metrics
        for record in records
        if isinstance((metrics := record.get("a5b_metrics")), dict)
    ]
    if not parsed:
        return None

    complete = [metrics for metrics in parsed if metrics.get("complete") is True]
    ranges: dict[str, dict[str, int]] = {}
    for metrics in complete:
        for section, values in metrics.items():
            if section in {"complete", "missing"} or not isinstance(values, dict):
                continue
            for field, value in values.items():
                if not isinstance(value, int):
                    continue
                key = f"{section}.{field}"
                aggregate = ranges.setdefault(key, {"min": value, "max": value})
                aggregate["min"] = min(aggregate["min"], value)
                aggregate["max"] = max(aggregate["max"], value)

    return {
        "runs_with_markers": len(parsed),
        "complete_runs": len(complete),
        "missing_runs": len(parsed) - len(complete),
        "ranges": ranges,
    }


def classify(
    log: bytes,
    profile: str,
    stage: str,
    required_ap_mode: str | None = None,
    require_contract: bool = False,
    max_runner_step_ms: int | None = None,
    qemu_contract_fixture: bool = False,
) -> str:
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

    if required_ap_mode is not None:
        observed_ap_mode = detected_ap_mode(log)
        if observed_ap_mode is not None and observed_ap_mode != required_ap_mode:
            return "wrong_ap_mode"
        success_evidence = (
            b"W2E_WPA3_CONNECT_OK" in log
            or b"RFDBG_A5B_CONNECT_PROFILE_OK" in log
            or bool(parse_ping_summaries(log))
        )
        if observed_ap_mode is None and success_evidence:
            return "missing_ap_mode"

    if b"RFDBG_A5B_CONNECT_PROFILE_OK" in log:
        metrics = parse_a5b_metrics(
            log,
            require_disconnect=stage == "connect",
        )
        if metrics is None or metrics.get("complete") is not True:
            return "missing_a5b_metrics"

    def pass_or_contract_violation() -> str:
        if require_contract and validate_rust_contract(
            log, stage, max_runner_step_ms, qemu_contract_fixture
        ):
            return "contract_violation"
        return "pass"

    if stage == "init-scan" and (
        b"RF3_SCAN_OK" in log or b"RFDBG_A5B_SCAN_PROFILE_OK" in log
    ):
        return pass_or_contract_violation()
    if stage == "connect" and b"RF5B_WPA_CONNECT_OK" in log:
        return pass_or_contract_violation()
    if stage == "connect" and (
        b"W2D_WPA2_CONNECT_OK" in log or b"W2E_WPA3_CONNECT_OK" in log
    ):
        return pass_or_contract_violation()
    if stage == "connect" and b"RFDBG_A5B_CONNECT_PROFILE_OK" in log:
        return pass_or_contract_violation()
    if b"RF5C_LOCAL_DATA_PATH_OK" in log:
        return pass_or_contract_violation()
    if b"RF5C_LOCAL_DATA_PATH_ERR" in log:
        return "local_data_path_failure"
    if b"RF5B_WPA_CONNECT_ERR:0x00001451" in log:
        return "auth_rsp2_timeout"
    if (
        b"RF5B_WPA_CONNECT_ERR:" in log
        or b"RF5B_CONNECT_ERR:" in log
        or b"W2D_WPA2_CONNECT_ERR" in log
        or b"W2E_WPA3_CONNECT_ERR" in log
        or b"RFDBG_A5B_CONNECT_ERR" in log
        or b"A4_NET_ERR" in log
    ):
        return "connect_error"
    if b"RF5B_AP_NOT_FOUND" in log:
        return "ap_not_found"
    if b"RF3_SCAN_ERR" in log or b"RFDBG_A5B_SCAN_ERR" in log:
        return "scan_error"
    if b"RF2_INIT_ERR:" in log:
        return "init_error"
    if b"RFDBG_EXCEPTION" in log:
        return "exception"
    return "capture_timeout"


def post_terminal_elapsed(
    terminal_seen_at: float | None, post_terminal_seconds: float, now: float
) -> bool:
    return (
        terminal_seen_at is not None
        and now - terminal_seen_at >= post_terminal_seconds
    )


def capture_run(
    port: serial.Serial,
    jlink: str,
    timeout: float,
    post_terminal_seconds: float,
    profile: str,
    stage: str,
) -> tuple[bytes, dict[str, float]]:
    port.reset_input_buffer()
    started = time.monotonic()
    pulse_nrst(jlink)
    log = bytearray()
    marker_times: dict[str, float] = {}
    deadline = started + timeout
    terminal_seen_at: float | None = None
    boot_seen = False
    boot_marker = b"boot.\r\n"
    reconnects = 0

    timed_markers = OFFICIAL_TIMED_MARKERS if profile == "official-liteos" else RUST_TIMED_MARKERS
    terminal_markers = OFFICIAL_TERMINAL_MARKERS
    if profile == "rust":
        failure_markers = tuple(
            marker
            for marker in RUST_TERMINAL_MARKERS
            if marker
            not in (
                b"RF5C_CONNECTIVITY_SUMMARY",
                b"RFDBG_A5B_CONNECT_PROFILE_OK",
            )
        )
        if stage == "init-scan":
            terminal_markers = (
                *failure_markers,
                b"RF3_SCAN_OK",
                b"RFDBG_A5B_SCAN_PROFILE_OK",
            )
        elif stage == "connect":
            terminal_markers = (
                *failure_markers,
                b"RFDBG_A5B_CONNECT_PROFILE_OK",
                b"RF5B_WPA_CONNECT_OK",
                b"W2D_WPA2_CONNECT_OK",
                b"W2E_WPA3_CONNECT_OK",
            )
        else:
            # The final connectivity contract includes lease renewal. A ping
            # summary is therefore progress, not a terminal success marker.
            terminal_markers = (*failure_markers, b"A4_DHCP_RENEW_OK")
    elif stage == "connect":
        success_marker = (
            b"+NOTICE:CONNECTED"
        )
        terminal_markers = (*terminal_markers, success_marker)

    while time.monotonic() < deadline:
        if post_terminal_elapsed(
            terminal_seen_at, post_terminal_seconds, time.monotonic()
        ):
            break
        try:
            chunk = port.read(4096)
        except serial.SerialException:
            reconnects += 1
            if reconnects > 3:
                raise
            port.close()
            reopen_deadline = time.monotonic() + 2.0
            while True:
                try:
                    port.open()
                    port.reset_input_buffer()
                    break
                except (OSError, serial.SerialException):
                    if time.monotonic() >= reopen_deadline:
                        raise
                    time.sleep(0.05)
            # The disconnect can lose the first bytes after nRST. Restart this
            # uncounted attempt only after the serial transport is stable, so a
            # host USB transient is neither a firmware failure nor a false pass.
            started = time.monotonic()
            deadline = started + timeout
            terminal_seen_at = None
            boot_seen = False
            log.clear()
            marker_times.clear()
            pulse_nrst(jlink)
            continue
        if not chunk:
            continue
        log.extend(chunk)
        if not boot_seen:
            boot_start = log.find(boot_marker)
            if boot_start < 0:
                continue
            # Bytes can still arrive from the previous firmware after the UART
            # input buffer is reset but before nRST reaches the target.  Keep
            # only this boot so stale success markers cannot terminate or pass
            # the next run.
            del log[:boot_start]
            boot_seen = True
        now = time.monotonic() - started
        for marker in timed_markers:
            name = marker.decode()
            if name not in marker_times and marker in log:
                marker_times[name] = round(now, 3)
        if terminal_seen_at is None and any(marker in log for marker in terminal_markers):
            terminal_seen_at = time.monotonic()

    return bytes(log), marker_times


def record_from_log(
    run: int,
    log: bytes,
    profile: str,
    stage: str,
    required_ap_mode: str | None,
    log_name: str,
    marker_times: dict[str, float] | None = None,
    require_contract: bool = False,
    max_runner_step_ms: int | None = None,
    qemu_contract_fixture: bool = False,
) -> dict[str, object]:
    contract_violations = (
        validate_rust_contract(
            log, stage, max_runner_step_ms, qemu_contract_fixture
        )
        if require_contract and profile == "rust"
        else []
    )
    public_ping = parse_ping_summaries(log).get(PUBLIC_ICMP_OBSERVATION_TARGET)
    public_icmp_observation = "missing"
    if public_ping is not None:
        public_icmp_observation = (
            "ok" if int(public_ping.get("rx", 0)) > 0 else "public_icmp_loss"
        )
    return {
        "run": run,
        "result": classify(
            log,
            profile,
            stage,
            required_ap_mode,
            require_contract,
            max_runner_step_ms,
            qemu_contract_fixture,
        ),
        "bytes": len(log),
        "auth_rsp2_timeouts": log.count(AUTH_RSP2_TIMEOUT_EVENT)
        + log.count(OFFICIAL_AUTH_RSP2_TIMEOUT_EVENT),
        "ap_mode": detected_ap_mode(log),
        "ping": parse_ping_summaries(log),
        "public_icmp_observation": public_icmp_observation,
        "l2_protocol": parse_l2_protocol_diagnostics(log),
        "a5b_metrics": parse_a5b_metrics(
            log,
            require_disconnect=stage == "connect",
        ),
        "contract": {
            "schema": MARKER_CONTRACT,
            "required": require_contract,
            "evidence_scope": (
                "contract-only" if qemu_contract_fixture else "silicon"
            ),
            "violations": contract_violations,
        },
        "marker_seconds": marker_times or {},
        "log": log_name,
    }


def summarize_records(
    records: list[dict[str, object]],
    *,
    port: str | None,
    baud: int,
    profile: str,
    stage: str,
    required_ap_mode: str | None,
    timeout: float,
    post_terminal_seconds: float,
    reference_ping: dict[str, object] | None,
    source_dir: Path | None = None,
) -> dict[str, object]:
    counts: dict[str, int] = {}
    public_icmp_observations: dict[str, int] = {}
    for record in records:
        result = str(record["result"])
        counts[result] = counts.get(result, 0) + 1
        observation = str(record.get("public_icmp_observation", "missing"))
        public_icmp_observations[observation] = (
            public_icmp_observations.get(observation, 0) + 1
        )

    ping_totals: dict[str, dict[str, int]] = {}
    for record in records:
        ping = record.get("ping", {})
        if not isinstance(ping, dict):
            continue
        for target, metrics in ping.items():
            if not isinstance(metrics, dict):
                continue
            totals = ping_totals.setdefault(
                str(target), {"tx": 0, "rx": 0, "drop": 0, "tx_error": 0}
            )
            for field in totals:
                value = metrics.get(field, 0)
                if isinstance(value, int):
                    totals[field] += value
    for totals in ping_totals.values():
        totals["loss_pct"] = (
            totals["drop"] * 100 // totals["tx"] if totals["tx"] else 100
        )

    summary: dict[str, object] = {
        "port": port,
        "baud": baud,
        "profile": profile,
        "stage": stage,
        "required_ap_mode": required_ap_mode,
        "runs": len(records),
        "timeout_seconds": timeout,
        "post_terminal_seconds": post_terminal_seconds,
        "counts": counts,
        "public_icmp_observations": public_icmp_observations,
        "ping_totals": ping_totals,
        "auth_rsp2_timeouts": sum(
            int(record["auth_rsp2_timeouts"]) for record in records
        ),
        "reference_ping": reference_ping,
        "a5b_metrics": aggregate_a5b_metrics(records),
        "records": records,
    }
    if source_dir is not None:
        summary["source_dir"] = str(source_dir.resolve())
    return summary


def print_record_failures(records: list[dict[str, object]]) -> None:
    for record in records:
        if record.get("result") == "pass":
            continue
        contract = record.get("contract")
        violations = (
            contract.get("violations", [])
            if isinstance(contract, dict)
            else []
        )
        print(
            f"run {record.get('run')}: result={record.get('result')} "
            f"contract_violations={json.dumps(violations)}",
            flush=True,
        )


def main() -> int:
    args = parse_args()
    if args.write_artifact_identity is not None:
        if args.elf is None or args.profile_id is None:
            raise SystemExit(
                "--write-artifact-identity requires --elf and --profile-id"
            )
        identity = write_artifact_identity(
            args.write_artifact_identity, args.elf, args.profile_id
        )
        print(
            "artifact identity: "
            f"{args.write_artifact_identity} ({identity['elf_sha256']})"
        )
        return 0

    if (
        args.runs <= 0
        or args.timeout <= 0
        or args.post_terminal_seconds < 0
        or args.reference_count <= 0
    ):
        raise SystemExit(
            "--runs, --timeout and --reference-count must be positive; "
            "--post-terminal-seconds must be non-negative"
        )
    if args.required_ap_mode is not None and args.profile != "rust":
        raise SystemExit("--required-ap-mode is only valid with --profile rust")
    if args.max_runner_step_ms is not None and args.max_runner_step_ms <= 0:
        raise SystemExit("--max-runner-step-ms must be positive")
    identity = None
    if args.artifact_identity is not None:
        if args.elf is None or args.profile_id is None:
            raise SystemExit("--artifact-identity requires --elf and --profile-id")
        try:
            identity = verify_artifact_identity(
                args.artifact_identity, args.elf, args.profile_id
            )
        except ValueError as error:
            raise SystemExit(str(error)) from error
    elif args.elf is not None or args.profile_id is not None:
        raise SystemExit("--elf/--profile-id require --artifact-identity")
    if args.analyze_dir is None and args.port is None:
        raise SystemExit("--port or PORT is required unless --analyze-dir is used")

    timestamp = time.strftime("%Y%m%d-%H%M%S")
    if args.analyze_dir is not None:
        source_logs = sorted(args.analyze_dir.glob("run-*.uart.log"))
        if not source_logs:
            raise SystemExit(f"no run-*.uart.log files in {args.analyze_dir}")
        output = args.output or args.analyze_dir / f"reanalysis-{timestamp}"
        output.mkdir(parents=True, exist_ok=False)
        records = [
            record_from_log(
                run,
                log_path.read_bytes(),
                args.profile,
                args.stage,
                args.required_ap_mode,
                log_path.name,
                require_contract=args.require_contract,
                max_runner_step_ms=args.max_runner_step_ms,
                qemu_contract_fixture=args.qemu_contract_fixture,
            )
            for run, log_path in enumerate(source_logs, start=1)
        ]
        summary = summarize_records(
            records,
            port=None,
            baud=args.baud,
            profile=args.profile,
            stage=args.stage,
            required_ap_mode=args.required_ap_mode,
            timeout=args.timeout,
            post_terminal_seconds=args.post_terminal_seconds,
            reference_ping=None,
            source_dir=args.analyze_dir,
        )
        summary["artifact_identity"] = identity
        summary["evidence_scope"] = (
            "contract-only" if args.qemu_contract_fixture else "silicon"
        )
        (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(f"summary: {json.dumps(summary['counts'], sort_keys=True)}")
        print_record_failures(records)
        print(f"artifacts: {output}")
        counts = summary["counts"]
        assert isinstance(counts, dict)
        return 0 if counts.get("pass", 0) == len(records) else 1

    output = (
        args.output
        or Path("/private/tmp") / f"ws63-connectivity-reset-matrix-{timestamp}"
    )
    output.mkdir(parents=True, exist_ok=False)
    records: list[dict[str, object]] = []
    reference_ping = None
    if args.reference_target:
        reference_ping = run_reference_ping(
            args.reference_target, args.reference_count, output / "reference-ping.log"
        )
        print(f"reference ping: {json.dumps(reference_ping, sort_keys=True)}", flush=True)

    # Open UART before every reset pulse so boot and early failure markers are
    # observable. Keep the descriptor open across runs to avoid driver churn.
    assert args.port is not None
    with serial.Serial(args.port, args.baud, timeout=0.2) as port:
        for run in range(1, args.runs + 1):
            log, marker_times = capture_run(
                port,
                args.jlink,
                args.timeout,
                args.post_terminal_seconds,
                args.profile,
                args.stage,
            )
            log_path = output / f"run-{run:02d}.uart.log"
            log_path.write_bytes(log)
            record = record_from_log(
                run,
                log,
                args.profile,
                args.stage,
                args.required_ap_mode,
                log_path.name,
                marker_times,
                args.require_contract,
                args.max_runner_step_ms,
                args.qemu_contract_fixture,
            )
            records.append(record)
            print(
                f"run {run:02d}/{args.runs}: {record['result']} "
                f"(auth_rsp2_timeouts={record['auth_rsp2_timeouts']}, {len(log)} bytes)",
                flush=True,
            )

    summary = summarize_records(
        records,
        port=args.port,
        baud=args.baud,
        profile=args.profile,
        stage=args.stage,
        required_ap_mode=args.required_ap_mode,
        timeout=args.timeout,
        post_terminal_seconds=args.post_terminal_seconds,
        reference_ping=reference_ping,
    )
    summary["artifact_identity"] = identity
    summary["evidence_scope"] = (
        "contract-only" if args.qemu_contract_fixture else "silicon"
    )
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    counts = summary["counts"]
    assert isinstance(counts, dict)
    print(f"summary: {json.dumps(counts, sort_keys=True)}")
    print_record_failures(records)
    print(f"artifacts: {output}")
    return 0 if counts.get("pass", 0) == args.runs else 1


if __name__ == "__main__":
    raise SystemExit(main())
