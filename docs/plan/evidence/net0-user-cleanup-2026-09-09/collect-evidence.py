#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Preserve successful and failed NET0 bring-up attempts with bounded public excerpts."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def match_one(pattern, content):
    found = [m for line in content.splitlines() if (m := re.fullmatch(pattern, line))]
    return found[0] if len(found) == 1 else None


def ci_for(source, run_id):
    ci = json.loads(subprocess.check_output([
        "gh", "run", "view", str(run_id), "--repo", "hispark-rs/hisi-rf-ws63",
        "--json", "headSha,status,conclusion,url,jobs"], text=True))
    if (ci["headSha"] != source or ci["conclusion"] != "success"
            or len(ci["jobs"]) != 23 or any(j["conclusion"] != "success" for j in ci["jobs"])):
        raise ValueError("Exact-source CI is not fully successful")
    return {k: ci[k] for k in ("headSha", "status", "conclusion", "url")} | {
        "jobs": [{k: j[k] for k in ("name", "conclusion")} for j in ci["jobs"]]}


PAYLOAD = (rb"RFDBG_NET0_PAYLOAD sent=0x([0-9a-f]{8}) received=0x([0-9a-f]{8})"
           rb" bitmap=0x([0-9a-f]{8}) invalid=0x([0-9a-f]{8}) duplicate=0x([0-9a-f]{8})")
MARKERS = re.compile(
    rb"(?:RFDBG_NET0_(?:BOUND_CLOSED|INITIAL_SESSION_(?:OPEN|CLOSED|REJECTED)|PAYLOAD_OK)"
    rb"|RFDBG_NET0_(?:INITIAL_RESULT result=|PAYLOAD_ERR reason=)[a-z-]+(?:_[a-z-]+)*"
    rb"|RFDBG_NET0_PAYLOAD(?: [a-z_]+=0x[0-9a-f]{8}){5}"
    rb"|RFDBG_NET0_ROUTE(?: [a-z_]+=[0-9a-f]{16}){6}"
    rb"|RFDBG_NET0_RX(?: [a-z_]+=[0-9a-f]{8}){8}"
    rb"|RFDBG_NET0_HOST_DELIVERY phase=(?:bootstrap|connected|payload|disconnected)"
    rb"(?: [a-z_]+=0x[0-9a-f]{16}){8} exhausted=[01]"
    rb"|RFDBG_A5B_(?:CONNECT|DISCONNECT)_OK elapsed_ms=0x[0-9a-f]{8}"
    rb"|RFDBG_A5B_CONNECT_PROFILE_OK"
    rb"|RFDBG_NET0_USER_CLEANUP(?: 0x[0-9a-f]{8}){10}"
    rb"|RFDBG_NET0_CLEANUP_FAULT_REJECTED"
    rb"|RFDBG_A5B_DISCONNECT_ERR code=wifi.connection_failed stage=disconnect backend=0x[0-9a-f]{8}"
    rb"|RFDBG_A5B_DISCONNECT_ERR_TRACE kind=hostap_status value=0x[0-9a-f]{8})"
)
PEER_MARKERS = re.compile(rb"(?:RFDBG_SOFTAP_READY|RFDBG_SOFTAP_NET(?: [a-z_]+=[0-9a-f]{8}){9})")


def cleanup_observation(data, negative):
    pattern = rb"RFDBG_NET0_USER_CLEANUP" + rb" 0x([0-9a-f]{8})" * 10
    samples = [[int(v, 16) for v in match.groups()] for line in data.splitlines()
               if (match := re.fullmatch(pattern, line))]
    code = 100 if negative else 0
    expected = [[0] * 10, [1, 1, 0, 1, 0, code, 0, 1, code, code]]
    panic = b"[PANIC]" in data or b"panicked at" in data
    result = {"samples": samples, "panic": panic, "pass": samples == expected and not panic}
    if negative:
        mapped = match_one(rb"RFDBG_A5B_DISCONNECT_ERR code=wifi.connection_failed stage=disconnect backend=0x([0-9a-f]{8})", data)
        raw = match_one(rb"RFDBG_A5B_DISCONNECT_ERR_TRACE kind=hostap_status value=0x([0-9a-f]{8})", data)
        result["mapped_status"] = int(mapped[1], 16) if mapped else None
        result["native_status"] = int(raw[1], 16) if raw else None
        result["pass"] &= (result["mapped_status"] == 0x5732d064 and result["native_status"] == 100
                           and b"RFDBG_NET0_CLEANUP_FAULT_REJECTED" in data.splitlines()
                           and not any(marker in data for marker in (
                               b"RFDBG_A5B_DISCONNECT_OK", b"RFDBG_NET0_INITIAL_SESSION_CLOSED",
                               b"RFDBG_A5B_CONNECT_PROFILE_OK")))
    return result


def summarize_case(case, output):
    raw = Path(case["directory"])
    original = json.loads((raw / "summary.json").read_text())
    if not case["source"].startswith(original["source_commit"]) or len(original["source_commit"]) < 7:
        raise ValueError("Capture source identity mismatch")
    if sha(Path(case["elf"])) != original["elf_sha256"]:
        raise ValueError("Captured ELF hash differs from preserved bytes")
    matrix = {k: original[k] for k in ("schema", "elf_sha256", "role", "boundary", "required_markers")}
    matrix.update(name=case["name"], source_commit=case["source"],
                  requested_rounds=case["requested_rounds"],
                  raw_summary_sha256=sha(raw / "summary.json"),
                  reset_policy=original.get("reset_policy", {"kind": "target-only"}),
                  interpretation=case["interpretation"], runs=[])
    if "flash" in original:
        matrix["flash"] = original["flash"] | {"log_sha256": sha(raw / "flash.log")}
    for run in original["runs"]:
        index = run["index"]
        contents = []
        excerpts = []
        for role, policy in (("target", MARKERS), ("peer", PEER_MARKERS)):
            path = raw / f"run-{index:02}.{role}.uart.log"
            data = path.read_bytes()
            capture = next(c for c in run["captures"] if c["role"] == role)
            if sha(path) != capture["sha256"] or len(data) != capture["bytes"]:
                raise ValueError("Raw UART evidence hash/length mismatch")
            contents.append(data)
            name = f"{case['name']}-run-{index:02}.{role}.markers.log"
            selected = [line for line in data.splitlines() if policy.fullmatch(line)]
            (output / name).write_bytes(b"\n".join(selected) + b"\n")
            excerpts.append({"file": name, "sha256": sha(output / name),
                             "policy": "full-line whitelist, never original UART"})
        data, peer = contents
        result = match_one(PAYLOAD, data)
        payload = None if result is None else dict(zip(
            ("sent", "received", "bitmap", "invalid", "duplicate"),
            (int(x, 16) for x in result.groups())))
        if payload != run.get("payload"):
            raise ValueError("Capture summary payload does not match UART")
        whole_lines = set(data.splitlines())
        control = all(match_one(rb"RFDBG_A5B_" + phase + rb"_OK elapsed_ms=0x[0-9a-f]{8}", data)
                      for phase in (b"CONNECT", b"DISCONNECT"))
        good = (control and payload is not None
                and payload["sent"] == payload["received"] == 10 and payload["bitmap"] == 1023
                and payload["invalid"] == 0
                and all(marker in whole_lines for marker in (
                    b"RFDBG_NET0_BOUND_CLOSED", b"RFDBG_NET0_INITIAL_SESSION_OPEN",
                    b"RFDBG_NET0_PAYLOAD_OK", b"RFDBG_NET0_INITIAL_SESSION_CLOSED",
                    b"RFDBG_A5B_CONNECT_PROFILE_OK")))
        negative = case.get("negative_cleanup", False)
        cleanup = cleanup_observation(data, negative)
        if negative:
            good = (bool(match_one(rb"RFDBG_A5B_CONNECT_OK elapsed_ms=0x[0-9a-f]{8}", data))
                    and payload == {"sent": 10, "received": 10, "bitmap": 1023, "invalid": 0, "duplicate": 0}
                    and cleanup["pass"] and all(marker in whole_lines for marker in (
                        b"RFDBG_NET0_BOUND_CLOSED", b"RFDBG_NET0_INITIAL_SESSION_OPEN", b"RFDBG_NET0_PAYLOAD_OK")))
        else:
            good = good and cleanup["pass"]
        if bool(good) != run["pass"]:
            raise ValueError("Run result does not match complete UART markers")
        initial = match_one(rb"RFDBG_NET0_INITIAL_RESULT result=([a-z-]+)", data)
        classification = ("cleanup-negative-pass" if negative and good else
                          "fixture-assertion-failed" if cleanup["panic"] else
                          "initial-session-cleanup-pass" if good else
                          "initial-session-rejected" if initial and initial[1] != b"open" else
                          "no-udp-reply" if payload and payload["received"] == 0 else
                          "control-or-payload-incomplete")
        row = dict(run, classification=classification, cleanup=cleanup, public_excerpts=excerpts,
                   flash_init_warning=b"Flash Init Fail! ret = 0x80001341" in data)
        route = match_one(rb"RFDBG_NET0_ROUTE(?: [a-z_]+=[0-9a-f]{16}){6}", data)
        if route:
            values = {k.decode(): int(v, 16) for k, v in re.findall(rb"([a-z_]+)=([0-9a-f]{16})", route[0])}
            if values["entered"] != values["queued"] + values["dropped"] + values["in_flight"]:
                raise ValueError("Callback admission conservation failed")
            row["route"] = values
        rx = match_one(rb"RFDBG_NET0_RX(?: [a-z_]+=[0-9a-f]{8}){8}", data)
        if rx:
            row["smoltcp_rx"] = {k.decode(): int(v, 16) for k, v in re.findall(rb"([a-z_]+)=([0-9a-f]{8})", rx[0])}
        peer_samples = [m for line in peer.splitlines() if (m := PEER_MARKERS.fullmatch(line))
                        and line.startswith(b"RFDBG_SOFTAP_NET ")]
        row["peer_echo_rx_tx_max"] = {
            key: max((int(re.search(key.encode() + rb"=([0-9a-f]{8})", m[0])[1], 16)
                      for m in peer_samples), default=None)
            for key in ("echo_rx", "echo_tx")}
        matrix["runs"].append(row)
    matrix["totals"] = {"rounds": len(matrix["runs"]),
        "passed": sum(r["pass"] for r in matrix["runs"]),
        "sent": sum((r.get("payload") or {}).get("sent", 0) for r in matrix["runs"]),
        "received": sum((r.get("payload") or {}).get("received", 0) for r in matrix["runs"])}
    return matrix


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    inputs = json.loads(args.inputs.read_text())
    args.output.mkdir(exist_ok=False)
    ci = [ci_for(item["source"], item["run"]) for item in inputs["ci"]]
    report = {"schema": "net0-cleanup-evidence/v1", "status": "experimental-bring-up",
        "boundary": "Initial-session traffic and checked HMAC cleanup status only; negative fixtures are not connectivity successes; native drainage, reconnect, Embassy Net and HTTPS remain unaccepted",
        "delivery": "Local ELF/image only; downloadable firmware bundle gate remains open",
        "ci": ci, "inputs": [], "matrices": []}
    for item in inputs["files"]:
        path = Path(item["path"])
        shutil.copyfile(path, args.output / item["name"])
        report["inputs"].append({"file": item["name"], "sha256": sha(path)})
    report["configuration"] = {"path": "examples/ws63/hil_wifi_config.rs",
        "sha256": sha(Path(inputs["configuration"])), "private_credentials_used": False,
        "kind": "committed public non-production paired fixture"}
    report["ap"] = {"elf_sha256": sha(Path(inputs["ap_elf"])),
        "prior_evidence": "../net0-control-2026-09-09/summary.json",
        "firmware_unchanged": True}
    for case in inputs["cases"]:
        report["matrices"].append(summarize_case(case, args.output))
    (args.output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    (args.output / "SHA256SUMS").write_text("".join(
        f"{sha(path)}  {path.name}\n" for path in sorted(args.output.iterdir())
        if path.is_file() and path.name != "SHA256SUMS"))
    print(json.dumps({m["name"]: m["totals"] for m in report["matrices"]}, indent=2))


if __name__ == "__main__":
    main()
