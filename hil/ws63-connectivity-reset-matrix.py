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
import time

import serial

from jlink_nrst import jlink_argv, pulse_nrst


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
    b"RF5C_PUBLIC_DNS_BEGIN",
    b"RF5C_PUBLIC_DNS_OK",
    b"RF5C_PUBLIC_DNS_SKIP",
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
MARKER_CONTRACT = "ws63-connectivity-markers/v3"

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
    b"RF5C_PUBLIC_DNS_ERR",
    b"RFDBG_EXCEPTION",
    b"panicked at",
    b"scheduler contract violation",
    b"BUDGET_VIOLATION",
)

QEMU_CONTRACT_FIXTURE_MARKER = b"RFDBG_CONNECTIVITY_CONTRACT_FIXTURE"
PUBLIC_DNS_TARGETS = ("223.5.5.5", "180.76.76.76")
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
IRQ45_LIFECYCLE_FIELDS = (
    "irq45_en_calls",
    "irq45_dis_calls",
    "irq45_clr_calls",
    "irq45",
    "irq45_enabled",
    "irq45_pending",
)
READY_OWNERSHIP_FIELDS = (
    "ready_owner_err",
    "ready_dup",
    "ready_wrong_bucket",
    "ready_bad_link",
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
        "--jlink-serial",
        default=os.environ.get("JLINK_SERIAL"),
        help="J-Link serial number; required to select the reset probe in a multi-rig setup",
    )
    parser.add_argument(
        "--peer-jlink-serial",
        help="optional peer/AP J-Link serial to reset before each measured target boot",
    )
    parser.add_argument(
        "--peer-port",
        help="optional peer/AP UART captured alongside each measured target boot",
    )
    parser.add_argument("--peer-baud", type=int, default=115_200)
    parser.add_argument(
        "--peer-settle-seconds",
        type=float,
        default=3.0,
        help="time allowed for the peer/AP to return before resetting the measured target",
    )
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
        "--require-resource-calibration",
        action="store_true",
        help=(
            "require the firmware resource contract and runtime heap metrics, "
            "with matching arenas and zero allocation failures"
        ),
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
    parser.add_argument(
        "--peer-artifact-identity",
        type=Path,
        help="verify the peer/AP identity manifest before paired-board capture",
    )
    parser.add_argument("--peer-elf", type=Path, help="peer/AP ELF covered by its identity")
    parser.add_argument("--peer-profile-id", help="peer/AP build profile identity")
    return parser.parse_args()


def parse_dns_summary(log: bytes) -> dict[str, int | str] | None:
    for line in reversed(log.splitlines()):
        if not line.startswith(
            (
                b"RF5C_PUBLIC_DNS_OK ",
                b"RF5C_PUBLIC_DNS_ERR ",
                b"RF5C_PUBLIC_DNS_SKIP ",
            )
        ):
            continue
        status = "ok" if line.startswith(b"RF5C_PUBLIC_DNS_OK ") else "error"
        if line.startswith(b"RF5C_PUBLIC_DNS_SKIP "):
            status = "skipped"
        fields: dict[str, int | str] = {"status": status}
        for token in line.decode(errors="replace").split()[1:]:
            if "=" not in token:
                continue
            key, value = token.split("=", 1)
            if key in ("target", "reason"):
                fields[key] = value
            elif value.lower().startswith("0x"):
                try:
                    fields[key] = int(value, 16)
                except ValueError:
                    continue
        return fields
    return None


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


def parse_text_fields(line: bytes) -> dict[str, str]:
    fields: dict[str, str] = {}
    for token in line.decode(errors="replace").split()[1:]:
        if "=" in token:
            key, value = token.split("=", 1)
            fields[key] = value
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


def parse_resource_calibration(log: bytes) -> dict[str, object] | None:
    resource_line = last_prefixed_line(log, b"RFDBG_RESOURCE ")
    heap_line = last_prefixed_line(log, b"RFDBG_HEAP ")
    if resource_line is None and heap_line is None:
        return None

    resource_text = parse_text_fields(resource_line or b"")
    resource = parse_hex_fields(resource_line or b"")
    heap = parse_hex_fields(heap_line or b"")
    violations: list[str] = []
    if resource_line is None:
        violations.append("missing:resource_contract")
    if heap_line is None:
        violations.append("missing:heap_metrics")

    resource_runtime = resource.get("runtime_arena", 0)
    resource_rf = resource.get("rf_arena", 0)
    if resource_line is not None:
        if not resource_text.get("schema"):
            violations.append("missing:resource.schema")
        if not resource_text.get("revision"):
            violations.append("missing:resource.revision")
        if resource_runtime <= 0:
            violations.append("invalid:resource.runtime_arena")
        if resource_rf <= 0:
            violations.append("invalid:resource.rf_arena")

    for prefix in ("rtos", "rf"):
        arena = heap.get(f"{prefix}_arena", 0)
        used = heap.get(f"{prefix}_used", 0)
        free = heap.get(f"{prefix}_free", 0)
        peak = heap.get(f"{prefix}_peak", 0)
        failures = heap.get(f"{prefix}_failures", 0)
        if heap_line is not None:
            if arena <= 0:
                violations.append(f"invalid:{prefix}.arena")
            if used + free != arena:
                violations.append(f"invalid:{prefix}.used_free")
            if peak < used or peak > arena:
                violations.append(f"invalid:{prefix}.peak")
            if failures != 0:
                violations.append(f"nonzero:{prefix}.failures")

    if resource_line is not None and heap_line is not None:
        if heap.get("rtos_arena") != resource_runtime:
            violations.append("mismatch:rtos.arena")
        if heap.get("rf_arena") != resource_rf:
            violations.append("mismatch:rf.arena")

    return {
        "resource": {**resource_text, **resource},
        "heap": heap,
        "violations": violations,
        "complete": not violations,
    }


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


def verify_optional_identity(
    manifest: Path | None,
    elf: Path | None,
    profile_id: str | None,
    *,
    label: str,
) -> dict[str, str] | None:
    provided = (manifest is not None, elf is not None, profile_id is not None)
    if not any(provided):
        return None
    if not all(provided):
        raise ValueError(
            f"{label} identity requires manifest, ELF and profile id together"
        )
    assert manifest is not None and elf is not None and profile_id is not None
    return verify_artifact_identity(manifest, elf, profile_id)


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


def parse_ready_ownership(log: bytes) -> dict[str, int] | None:
    line = last_prefixed_line(log, b"RFDBG_A5B_SCHED ")
    if line is None:
        line = last_prefixed_line(log, b"RFDBG_SOFTAP_SCHED ")
    return parse_hex_fields(line) if line is not None else None


def validate_ready_ownership(log: bytes, role: str) -> list[str]:
    snapshot = parse_ready_ownership(log)
    if snapshot is None:
        return [f"missing:{role}.ready_ownership"]

    violations: list[str] = []
    for field in READY_OWNERSHIP_FIELDS:
        if field not in snapshot:
            violations.append(f"missing:{role}.ready_ownership.{field}")
        elif snapshot[field] != 0:
            violations.append(
                f"nonzero:{role}.ready_ownership.{field}={snapshot[field]}"
            )
    return violations


def validate_rust_contract(
    log: bytes,
    stage: str,
    max_runner_step_ms: int | None = None,
    qemu_contract_fixture: bool = False,
    require_resource_calibration: bool = False,
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
        violations.extend(validate_ready_ownership(log, "sta"))
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
        if not any(
            marker in log
            for marker in (b"RF5C_PUBLIC_DNS_OK", b"RF5C_PUBLIC_DNS_SKIP")
        ):
            violations.append("missing:public_dns")
        if (
            b"W2E_WPA3_CONNECT_OK" in log
            and b"W2E_WPA3_CONNECT_OK pmf=required" not in log
        ):
            violations.append("invalid:wpa3.pmf")
        public_dns_skipped = b"RF5C_PUBLIC_DNS_SKIP reason=no-default-route" in log
        summary_line = last_prefixed_line(log, b"RF5C_CONNECTIVITY_SUMMARY ")
        if summary_line is not None:
            summary = parse_hex_fields(summary_line)
            for field in (
                "arp_request",
                "arp_reply",
                "dns_attempts",
                "dns_responses",
                "dns_invalid",
                "dns_tx_error",
                "rx_queue_drop",
            ):
                if field not in summary:
                    violations.append(f"missing:summary.{field}")
            if summary.get("arp_request", 0) <= 0:
                violations.append("invalid:summary.arp_request")
            if summary.get("arp_reply", 0) <= 0:
                violations.append("invalid:summary.arp_reply")
            if public_dns_skipped:
                if summary.get("dns_attempts", 0) != 0:
                    violations.append("nonzero:summary.dns_attempts")
                if summary.get("dns_responses", 0) != 0:
                    violations.append("nonzero:summary.dns_responses")
            else:
                if summary.get("dns_attempts", 0) <= 0:
                    violations.append("invalid:summary.dns_attempts")
                if summary.get("dns_responses", 0) <= 0:
                    violations.append("invalid:summary.dns_responses")
            if summary.get("dns_tx_error", 0) != 0:
                violations.append("nonzero:summary.dns_tx_error")
            if summary.get("rx_queue_drop", 0) != 0:
                violations.append("nonzero:summary.rx_queue_drop")

        dns = parse_dns_summary(log)
        if dns is not None:
            if dns.get("status") == "skipped":
                if dns.get("reason") != "no-default-route":
                    violations.append("invalid:public_dns.skip_reason")
            else:
                if dns.get("target") not in PUBLIC_DNS_TARGETS:
                    violations.append("invalid:public_dns.target")
                if dns.get("status") != "ok":
                    violations.append("invalid:public_dns.status")
                if int(dns.get("attempts", 0)) <= 0:
                    violations.append("invalid:public_dns.attempts")
                if int(dns.get("responses", 0)) <= 0:
                    violations.append("invalid:public_dns.responses")
                if int(dns.get("tx_error", 0)) != 0:
                    violations.append("nonzero:public_dns.tx_error")

        renew_line = last_prefixed_line(log, b"A4_DHCP_RENEW_OK ")
        if renew_line is not None:
            renew = parse_hex_fields(renew_line)
            if renew.get("client", 0) <= 0:
                violations.append("invalid:dhcp_renew.client")
            if renew.get("server", 0) <= 0:
                violations.append("invalid:dhcp_renew.server")

    if require_resource_calibration:
        calibration = parse_resource_calibration(log)
        if calibration is None:
            violations.append("missing:resource_calibration")
        else:
            violations.extend(str(item) for item in calibration["violations"])

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


def parse_local_echo_path(log: bytes) -> list[dict[str, int | str]] | None:
    """Parse per-attempt MAC-to-Rust receive-path snapshots."""
    samples = []
    for line in log.splitlines():
        if not line.startswith(b"RFDBG_LOCAL_ECHO_PATH "):
            continue
        sample: dict[str, int | str] = parse_hex_fields(line)
        phase = parse_text_fields(line).get("phase")
        if phase is not None:
            sample["phase"] = phase
        samples.append(sample)
    return samples or None


def parse_softap_echo_path(log: bytes) -> list[dict[str, object]] | None:
    """Parse per-echo SoftAP TX submission/completion snapshots."""
    samples: list[dict[str, object]] = []
    for line in log.splitlines():
        if not line.startswith(b"RFDBG_SOFTAP_ECHO_PATH "):
            continue
        sample: dict[str, object] = parse_hex_fields(line)
        encoded_statuses = parse_text_fields(line).get("tx_status_delta", "")
        statuses = []
        for encoded in encoded_statuses.split(","):
            if not encoded:
                continue
            try:
                statuses.append(int(encoded, 16))
            except ValueError:
                statuses = []
                break
        sample["tx_status_delta"] = statuses
        samples.append(sample)
    return samples or None


def parse_irq45_lifecycle(log: bytes, prefix: bytes) -> dict[str, int] | None:
    """Parse the optional WLMAC IRQ lifecycle snapshot from one trailer."""
    line = last_prefixed_line(log, prefix)
    if line is None:
        return None
    fields = parse_hex_fields(line)
    if not all(field in fields for field in IRQ45_LIFECYCLE_FIELDS):
        return None
    return {field: fields[field] for field in IRQ45_LIFECYCLE_FIELDS}


def correlate_echo_paths(
    sta_samples: object, ap_samples: object
) -> dict[str, object] | None:
    """Correlate sequence-tagged STA sends/receives with SoftAP TX evidence."""
    if not isinstance(sta_samples, list) or not isinstance(ap_samples, list):
        return None

    def sequences(samples: list[object], phase: str | None = None) -> set[int]:
        values = set()
        for sample in samples:
            if not isinstance(sample, dict):
                continue
            if phase is not None and sample.get("phase") != phase:
                continue
            sequence = sample.get("sequence")
            if isinstance(sequence, int):
                values.add(sequence)
        return values

    sent = sequences(sta_samples, "send")
    received = sequences(sta_samples, "receive")
    ap_observed = sequences(ap_samples)
    ap_submitted = {
        int(sample["sequence"])
        for sample in ap_samples
        if isinstance(sample, dict)
        and isinstance(sample.get("sequence"), int)
        and isinstance(sample.get("vendor_tx_delta"), int)
        and int(sample["vendor_tx_delta"]) > 0
    }
    return {
        "sta_sent": sorted(sent),
        "sta_received": sorted(received),
        "ap_observed": sorted(ap_observed),
        "ap_submitted": sorted(ap_submitted),
        "sent_missing_at_ap": sorted(sent - ap_observed),
        "ap_without_sta_send": sorted(ap_observed - sent),
        "submitted_without_sta_receive": sorted(ap_submitted - received),
    }


def aggregate_echo_correlation(
    records: list[dict[str, object]],
) -> dict[str, object] | None:
    """Summarize sequence ownership across both UART captures."""
    parsed = [
        (str(record.get("result", "unknown")), correlation)
        for record in records
        if isinstance((correlation := record.get("echo_correlation")), dict)
    ]
    if not parsed:
        return None

    fields = (
        "sta_sent",
        "sta_received",
        "ap_observed",
        "ap_submitted",
        "sent_missing_at_ap",
        "submitted_without_sta_receive",
    )

    def totals(entries: list[dict[str, object]]) -> dict[str, int]:
        return {
            field: sum(
                len(value)
                for entry in entries
                if isinstance((value := entry.get(field)), list)
            )
            for field in fields
        }

    grouped: dict[str, list[dict[str, object]]] = {}
    for result, correlation in parsed:
        grouped.setdefault(result, []).append(correlation)

    return {
        "runs_with_markers": len(parsed),
        "runs_without_markers": len(records) - len(parsed),
        "totals": totals([correlation for _, correlation in parsed]),
        "by_result": {
            result: {"runs": len(entries), **totals(entries)}
            for result, entries in sorted(grouped.items())
        },
    }


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


def aggregate_l2_protocol_diagnostics(
    records: list[dict[str, object]],
) -> dict[str, object] | None:
    parsed = [
        diagnostics
        for record in records
        if isinstance((diagnostics := record.get("l2_protocol")), dict)
    ]
    if not parsed:
        return None

    ranges: dict[str, dict[str, int]] = {}
    for field in L2_PROTOCOL_FIELDS:
        values = [
            value
            for diagnostics in parsed
            if isinstance((value := diagnostics.get(field)), int)
        ]
        if values:
            ranges[field] = {"min": min(values), "max": max(values)}

    return {
        "runs_with_markers": len(parsed),
        "runs_without_markers": len(records) - len(parsed),
        "ranges": ranges,
    }


def aggregate_local_echo_path(
    records: list[dict[str, object]],
) -> dict[str, object] | None:
    parsed = [
        samples
        for record in records
        if isinstance((samples := record.get("local_echo_path")), list)
    ]
    if not parsed:
        return None

    delta_ranges: dict[str, dict[str, int]] = {}
    for samples in parsed:
        for previous, current in zip(samples, samples[1:]):
            if not isinstance(previous, dict) or not isinstance(current, dict):
                continue
            for field, current_value in current.items():
                previous_value = previous.get(field)
                if field == "sequence" or not isinstance(previous_value, int):
                    continue
                delta = (current_value - previous_value) & 0xFFFF_FFFF
                aggregate = delta_ranges.setdefault(
                    field, {"min": delta, "max": delta}
                )
                aggregate["min"] = min(aggregate["min"], delta)
                aggregate["max"] = max(aggregate["max"], delta)

    sample_counts = [len(samples) for samples in parsed]
    return {
        "runs_with_markers": len(parsed),
        "runs_without_markers": len(records) - len(parsed),
        "samples": {"min": min(sample_counts), "max": max(sample_counts)},
        "step_delta_ranges": delta_ranges,
    }


def aggregate_irq45_lifecycle(
    records: list[dict[str, object]], key: str
) -> dict[str, object] | None:
    parsed = [
        diagnostics
        for record in records
        if isinstance((diagnostics := record.get(key)), dict)
    ]
    if not parsed:
        return None

    return {
        "runs_with_markers": len(parsed),
        "runs_without_markers": len(records) - len(parsed),
        "disabled_runs": sum(value["irq45_enabled"] == 0 for value in parsed),
        "pending_runs": sum(value["irq45_pending"] != 0 for value in parsed),
        "ranges": {
            field: {
                "min": min(value[field] for value in parsed),
                "max": max(value[field] for value in parsed),
            }
            for field in IRQ45_LIFECYCLE_FIELDS
        },
    }


def classify(
    log: bytes,
    profile: str,
    stage: str,
    required_ap_mode: str | None = None,
    require_contract: bool = False,
    max_runner_step_ms: int | None = None,
    qemu_contract_fixture: bool = False,
    require_resource_calibration: bool = False,
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
            or parse_dns_summary(log) is not None
        )
        if observed_ap_mode is None and success_evidence:
            return "missing_ap_mode"

    if b"RF5C_LOCAL_DATA_PATH_ERR" in log:
        return "local_data_path_failure"
    if b"RF5C_PUBLIC_DNS_ERR" in log:
        return "public_dns_failure"
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

    if b"RFDBG_A5B_CONNECT_PROFILE_OK" in log:
        metrics = parse_a5b_metrics(
            log,
            require_disconnect=stage == "connect",
        )
        if metrics is None or metrics.get("complete") is not True:
            return "missing_a5b_metrics"

    def pass_or_contract_violation() -> str:
        if (require_contract or require_resource_calibration) and validate_rust_contract(
            log,
            stage,
            max_runner_step_ms,
            qemu_contract_fixture,
            require_resource_calibration,
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
    return "capture_timeout"


def post_terminal_elapsed(
    terminal_seen_at: float | None, post_terminal_seconds: float, now: float
) -> bool:
    return (
        terminal_seen_at is not None
        and now - terminal_seen_at >= post_terminal_seconds
    )


def drain_serial(port: serial.Serial | None) -> bytes:
    if port is None:
        return b""
    waiting = port.in_waiting
    # CH340 drivers can report zero queued bytes until a read advances the USB
    # receive path. The peer port has a short timeout, so an explicit bounded
    # read is safe and avoids silently producing empty peer evidence.
    return port.read(waiting if waiting else 4096)


def current_boot_log(log: bytes) -> bytes:
    boot_start = log.find(b"boot.\r\n")
    return log[boot_start:] if boot_start >= 0 else log


def measured_uart_logs(directory: Path) -> list[Path]:
    return sorted(
        path
        for path in directory.glob("run-*.uart.log")
        if ".peer.uart.log" not in path.name
    )


def capture_run(
    port: serial.Serial,
    peer_port: serial.Serial | None,
    jlink: str,
    jlink_serial: str | None,
    peer_jlink_serial: str | None,
    peer_settle_seconds: float,
    timeout: float,
    post_terminal_seconds: float,
    profile: str,
    stage: str,
) -> tuple[bytes, bytes, dict[str, float]]:
    port.reset_input_buffer()
    if peer_port is not None:
        peer_port.reset_input_buffer()
    peer_log = bytearray()
    if peer_jlink_serial is not None:
        pulse_nrst(jlink, peer_jlink_serial)
        settle_deadline = time.monotonic() + peer_settle_seconds
        while time.monotonic() < settle_deadline:
            peer_log.extend(drain_serial(peer_port))
            time.sleep(0.02)
    started = time.monotonic()
    pulse_nrst(jlink, jlink_serial)
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
            # The final connectivity contract includes lease renewal. A DNS
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
            peer_log.clear()
            marker_times.clear()
            if peer_jlink_serial is not None:
                if peer_port is not None:
                    peer_port.reset_input_buffer()
                pulse_nrst(jlink, peer_jlink_serial)
                settle_deadline = time.monotonic() + peer_settle_seconds
                while time.monotonic() < settle_deadline:
                    peer_log.extend(drain_serial(peer_port))
                    time.sleep(0.02)
            pulse_nrst(jlink, jlink_serial)
            continue
        peer_log.extend(drain_serial(peer_port))
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

    peer_log.extend(drain_serial(peer_port))
    return bytes(log), current_boot_log(bytes(peer_log)), marker_times


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
    require_resource_calibration: bool = False,
    peer_log: bytes | None = None,
    peer_log_name: str | None = None,
) -> dict[str, object]:
    contract_violations = (
        validate_rust_contract(
            log,
            stage,
            max_runner_step_ms,
            qemu_contract_fixture,
            require_resource_calibration,
        )
        if (require_contract or require_resource_calibration) and profile == "rust"
        else []
    )
    if (
        require_contract
        and peer_log is not None
        and b"RFDBG_SOFTAP_READY" not in peer_log
    ):
        contract_violations.append("missing peer RFDBG_SOFTAP_READY marker")
    if require_contract and peer_log is not None:
        contract_violations.extend(validate_ready_ownership(peer_log, "peer"))
    result = classify(
        log,
        profile,
        stage,
        required_ap_mode,
        require_contract,
        max_runner_step_ms,
        qemu_contract_fixture,
        require_resource_calibration,
    )
    if result == "pass" and contract_violations:
        result = "contract_violation"
    dns = parse_dns_summary(log)
    local_echo_path = parse_local_echo_path(log)
    peer_echo_path = parse_softap_echo_path(peer_log or b"")
    irq45_lifecycle = parse_irq45_lifecycle(log, b"RFDBG_A5B_DATA_PATH ")
    peer_irq45_lifecycle = parse_irq45_lifecycle(
        peer_log or b"", b"RFDBG_SOFTAP_STATE "
    )
    return {
        "run": run,
        "result": result,
        "bytes": len(log),
        "auth_rsp2_timeouts": log.count(AUTH_RSP2_TIMEOUT_EVENT)
        + log.count(OFFICIAL_AUTH_RSP2_TIMEOUT_EVENT),
        "ap_mode": detected_ap_mode(log),
        "dns": dns,
        "l2_protocol": parse_l2_protocol_diagnostics(log),
        "local_echo_path": local_echo_path,
        "irq45_lifecycle": irq45_lifecycle,
        "ready_ownership": parse_ready_ownership(log),
        "peer_bytes": len(peer_log) if peer_log is not None else None,
        "peer_ready": (
            b"RFDBG_SOFTAP_READY" in peer_log if peer_log is not None else None
        ),
        "peer_echo_path": peer_echo_path,
        "peer_irq45_lifecycle": peer_irq45_lifecycle,
        "peer_ready_ownership": parse_ready_ownership(peer_log or b""),
        "echo_correlation": correlate_echo_paths(local_echo_path, peer_echo_path),
        "a5b_metrics": parse_a5b_metrics(
            log,
            require_disconnect=stage == "connect",
        ),
        "resource_calibration": parse_resource_calibration(log),
        "contract": {
            "schema": MARKER_CONTRACT,
            "required": require_contract or require_resource_calibration,
            "evidence_scope": (
                "contract-only" if qemu_contract_fixture else "silicon"
            ),
            "violations": contract_violations,
        },
        "marker_seconds": marker_times or {},
        "log": log_name,
        "peer_log": peer_log_name,
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
    peer_port: str | None = None,
    peer_baud: int | None = None,
) -> dict[str, object]:
    counts: dict[str, int] = {}
    dns_observations: dict[str, int] = {}
    for record in records:
        result = str(record["result"])
        counts[result] = counts.get(result, 0) + 1
        dns = record.get("dns")
        observation = (
            str(dns.get("status", "missing"))
            if isinstance(dns, dict)
            else "missing"
        )
        dns_observations[observation] = dns_observations.get(observation, 0) + 1

    dns_totals = {
        "attempts": 0,
        "responses": 0,
        "invalid": 0,
        "tx_error": 0,
    }
    for record in records:
        dns = record.get("dns")
        if not isinstance(dns, dict):
            continue
        for field in dns_totals:
            value = dns.get(field, 0)
            if isinstance(value, int):
                dns_totals[field] += value

    calibrations = [
        value
        for record in records
        if isinstance((value := record.get("resource_calibration")), dict)
    ]
    resource_calibration = None
    if calibrations:
        heap_fields = (
            "rtos_used",
            "rtos_free",
            "rtos_peak",
            "rtos_allocs",
            "rtos_failures",
            "rf_used",
            "rf_free",
            "rf_peak",
            "rf_failures",
        )
        ranges = {
            field: {
                "min": min(int(value["heap"].get(field, 0)) for value in calibrations),
                "max": max(int(value["heap"].get(field, 0)) for value in calibrations),
            }
            for field in heap_fields
        }
        resource_calibration = {
            "runs_with_markers": len(calibrations),
            "runs_without_markers": len(records) - len(calibrations),
            "complete_runs": sum(value.get("complete") is True for value in calibrations),
            "resource": calibrations[0]["resource"],
            "ranges": ranges,
            "runtime_headroom_remaining_bytes": min(
                int(value["heap"].get("rtos_arena", 0))
                - int(value["heap"].get("rtos_peak", 0))
                for value in calibrations
            ),
            "rf_headroom_remaining_bytes": min(
                int(value["heap"].get("rf_arena", 0))
                - int(value["heap"].get("rf_peak", 0))
                for value in calibrations
            ),
        }

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
        "dns_observations": dns_observations,
        "dns_totals": dns_totals,
        "auth_rsp2_timeouts": sum(
            int(record["auth_rsp2_timeouts"]) for record in records
        ),
        "reference_ping": reference_ping,
        "peer_port": peer_port,
        "peer_baud": peer_baud,
        "peer_ready_runs": sum(record.get("peer_ready") is True for record in records),
        "a5b_metrics": aggregate_a5b_metrics(records),
        "l2_protocol": aggregate_l2_protocol_diagnostics(records),
        "local_echo_path": aggregate_local_echo_path(records),
        "echo_correlation": aggregate_echo_correlation(records),
        "irq45_lifecycle": aggregate_irq45_lifecycle(records, "irq45_lifecycle"),
        "peer_irq45_lifecycle": aggregate_irq45_lifecycle(
            records, "peer_irq45_lifecycle"
        ),
        "resource_calibration": resource_calibration,
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
    try:
        identity = verify_optional_identity(
            args.artifact_identity,
            args.elf,
            args.profile_id,
            label="target",
        )
        peer_identity = verify_optional_identity(
            args.peer_artifact_identity,
            args.peer_elf,
            args.peer_profile_id,
            label="peer",
        )
    except ValueError as error:
        raise SystemExit(str(error)) from error
    if args.analyze_dir is None and args.port is None:
        raise SystemExit("--port or PORT is required unless --analyze-dir is used")

    timestamp = time.strftime("%Y%m%d-%H%M%S")
    if args.analyze_dir is not None:
        source_logs = measured_uart_logs(args.analyze_dir)
        if not source_logs:
            raise SystemExit(f"no run-*.uart.log files in {args.analyze_dir}")
        output = args.output or args.analyze_dir / f"reanalysis-{timestamp}"
        output.mkdir(parents=True, exist_ok=False)
        records = []
        for run, log_path in enumerate(source_logs, start=1):
            peer_log_path = log_path.with_name(
                log_path.name.replace(".uart.log", ".peer.uart.log")
            )
            peer_log = peer_log_path.read_bytes() if peer_log_path.exists() else None
            records.append(
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
                    require_resource_calibration=args.require_resource_calibration,
                    peer_log=peer_log,
                    peer_log_name=(
                        peer_log_path.name if peer_log is not None else None
                    ),
                )
            )
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
            peer_port=args.peer_port,
            peer_baud=args.peer_baud if args.peer_port is not None else None,
        )
        summary["artifact_identity"] = identity
        summary["peer_artifact_identity"] = peer_identity
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
    peer_port = (
        serial.Serial(args.peer_port, args.peer_baud, timeout=0.02)
        if args.peer_port is not None
        else None
    )
    try:
        with serial.Serial(args.port, args.baud, timeout=0.2) as port:
            for run in range(1, args.runs + 1):
                log, peer_log, marker_times = capture_run(
                    port,
                    peer_port,
                    args.jlink,
                    args.jlink_serial,
                    args.peer_jlink_serial,
                    args.peer_settle_seconds,
                    args.timeout,
                    args.post_terminal_seconds,
                    args.profile,
                    args.stage,
                )
                log_path = output / f"run-{run:02d}.uart.log"
                log_path.write_bytes(log)
                peer_log_path = output / f"run-{run:02d}.peer.uart.log"
                if peer_port is not None:
                    peer_log_path.write_bytes(peer_log)
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
                    args.require_resource_calibration,
                    peer_log=peer_log if peer_port is not None else None,
                    peer_log_name=peer_log_path.name if peer_port is not None else None,
                )
                records.append(record)
                print(
                    f"run {run:02d}/{args.runs}: {record['result']} "
                    f"(auth_rsp2_timeouts={record['auth_rsp2_timeouts']}, {len(log)} bytes)",
                    flush=True,
                )
    finally:
        if peer_port is not None:
            peer_port.close()

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
        peer_port=args.peer_port,
        peer_baud=args.peer_baud if args.peer_port is not None else None,
    )
    summary["artifact_identity"] = identity
    summary["peer_artifact_identity"] = peer_identity
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
