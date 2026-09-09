#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Preserve NET0 host TX attempts without exporting unrestricted UART logs."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import unittest

HERE = Path(__file__).resolve().parent
SOURCE = "60faf85b7ce47c1c3f8a93f604c911785d3d671b"
ELF = "717f4d81ca3f2c6f050959a05c22c230ba9f11f5ff5ddba1188ddafcfd0bb026"
FIELDS = ("accepted", "processed", "dropped", "pending", "peak", "rejected", "callback_errors", "closed", "fault")
TX = re.compile(rb"RFDBG_NET0_HOST_TX" + rb" 0x([0-9a-f]{8})" * len(FIELDS))
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("prior", HERE.parent / "net0-user-cleanup-2026-09-09/collect-evidence.py")
prior = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prior)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def tx_contract(samples):
    if len(samples) != 2:
        return False
    for sample in samples:
        if (sample["accepted"] != sample["processed"] + sample["dropped"] + sample["pending"]
                or sample["fault"] or sample["callback_errors"]):
            return False
    first, last = samples
    return (last["accepted"] > 0 and last["accepted"] >= first["accepted"]
            and last["pending"] == 0 and last["closed"] == 1)


def collect_case(directory, requested, wrong_threshold=False):
    original = json.loads((directory / "summary.json").read_text())
    if original["elf_sha256"] != ELF:
        raise ValueError("Unexpected HIL ELF")
    result = {"name": directory.name, "requested": requested,
              "raw_summary_sha256": sha((directory / "summary.json").read_bytes()),
              "original_source_label": original["source_commit"], "runs": []}
    if "flash" in original:
        result["flash"] = original["flash"]
    for row in original["runs"]:
        public = {}
        for role in ("target", "peer"):
            data = (directory / f"run-{row['index']:02}.{role}.uart.log").read_bytes()
            capture = next(item for item in row["captures"] if item["role"] == role)
            if sha(data) != capture["sha256"] or len(data) != capture["bytes"]:
                raise ValueError("Raw capture does not match its original receipt")
            policy = prior.MARKERS if role == "target" else prior.PEER_MARKERS
            public[role] = [line.decode("ascii") for line in data.splitlines()
                            if policy.fullmatch(line) or (role == "target" and TX.fullmatch(line))]
            if role != "target":
                continue
            samples = [dict(zip(FIELDS, (int(value, 16) for value in match.groups())))
                       for line in data.splitlines() if (match := TX.fullmatch(line))]
            if samples != row["host_tx"]["samples"]:
                raise ValueError("Host TX summary disagrees with complete UART lines")
            payload = prior.match_one(prior.PAYLOAD, data)
            payload = None if payload is None else dict(zip(
                ("sent", "received", "bitmap", "invalid", "duplicate"),
                (int(value, 16) for value in payload.groups())))
            if payload != row.get("payload"):
                raise ValueError("Payload summary disagrees with UART")
            cleanup = prior.cleanup_observation(data, False)
            if cleanup["samples"] != row["cleanup"]["samples"]:
                raise ValueError("Cleanup summary disagrees with UART")
            control = all(prior.match_one(rb"RFDBG_A5B_" + phase + rb"_OK elapsed_ms=0x[0-9a-f]{8}", data)
                          for phase in (b"CONNECT", b"DISCONNECT"))
            good = (control and tx_contract(samples) and cleanup["pass"]
                    and payload == {"sent": 10, "received": 10, "bitmap": 1023, "invalid": 0, "duplicate": 0}
                    and all(marker in data.splitlines() for marker in (
                        b"RFDBG_NET0_PAYLOAD_OK", b"RFDBG_NET0_INITIAL_SESSION_CLOSED",
                        b"RFDBG_A5B_CONNECT_PROFILE_OK")))
            if bool(good) != row["pass"] and not wrong_threshold:
                raise ValueError("Recorded outcome does not match the declared contract")
            if wrong_threshold and not (good and not row["pass"] and samples[-1]["accepted"] < 10):
                raise ValueError("Incorrect-threshold case is not the preserved real failure")
        result["runs"].append(dict(row, public_markers=public,
                                  classification="capture-threshold-error" if wrong_threshold else
                                  "pass" if good else "firmware-or-contract-failure",
                                  corrected_contract_pass=bool(good)))
    result["recorded_passes"] = sum(row["pass"] for row in result["runs"])
    result["udp_sent"] = sum((row.get("payload") or {}).get("sent", 0) for row in result["runs"])
    result["udp_received"] = sum((row.get("payload") or {}).get("received", 0) for row in result["runs"])
    return result


class Tests(unittest.TestCase):
    def test_counter_gate(self):
        first = dict(zip(FIELDS, (2, 2, 0, 0, 1, 0, 0, 0, 0)))
        last = dict(first, closed=1)
        self.assertTrue(tx_contract([first, last]))
        for key, value in (("accepted", 3), ("pending", 1), ("closed", 0), ("fault", 100), ("callback_errors", 1)):
            self.assertFalse(tx_contract([first, dict(last, **{key: value})]))
        self.assertFalse(tx_contract([last]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw-root", type=Path)
    parser.add_argument("--backend", type=Path)
    parser.add_argument("--build-source", type=Path)
    parser.add_argument("--elf", type=Path)
    parser.add_argument("--test", action="store_true")
    args = parser.parse_args()
    if args.test:
        if not unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests)).wasSuccessful():
            raise SystemExit(1)
    if not args.raw_root:
        return
    if sha(args.elf.read_bytes()) != ELF:
        raise ValueError("Preserved ELF bytes changed")
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=args.backend, text=True).strip()
    if head != SOURCE:
        raise ValueError("Source commit mismatch")
    names = subprocess.check_output(["git", "ls-files", "-z", "src", "examples", "build.rs", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo"], cwd=args.backend).decode().split("\0")
    inputs = []
    for name in sorted(filter(None, names)):
        data = (args.backend / name).read_bytes()
        if data != (args.build_source / name).read_bytes():
            raise ValueError(f"Independent build source mismatch: {name}")
        inputs.append({"path": name, "sha256": sha(data)})
    cases = [collect_case(args.raw_root / "net0-host-tx-paired3-20260909", 3, True),
             collect_case(args.raw_root / "net0-host-tx-paired3-v2-20260909", 3),
             collect_case(args.raw_root / "net0-host-tx-paired20-60faf85", 20)]
    result = {"schema": "net0-host-tx-hil/v1", "source_commit": SOURCE,
              "elf_sha256": ELF, "build_inputs": inputs, "cases": cases,
              "boundary": "Host queue-4 drain and one-shot traffic only; not DMAC/RX producer fencing or reconnect"}
    plan_root = args.raw_root / "net0-host-tx-paired3-20260909/target/riscv32imfc-unknown-none-elf/release"
    result["image_sha256"] = sha((plan_root / "sta-host-tx.img").read_bytes())
    result["ap_elf_sha256"] = sha((args.raw_root / "net0-control-hil-3914ff5/ap.elf").read_bytes())
    if result["ap_elf_sha256"] != "d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091":
        raise ValueError("The fixed AP artifact changed")
    config = HERE.parents[3] / "examples/ws63/hil_wifi_config.rs"
    result["public_paired_config"] = {"path": "examples/ws63/hil_wifi_config.rs",
                                      "sha256": sha(config.read_bytes())}
    for source, target in ((plan_root / "sta-host-tx.plan.json", "flash-plan.json"),
                           (args.raw_root / "net0-host-tx-link.json", "native-calls.json"),
                           (args.raw_root / "net0-host-tx-storage.json", "storage.json"),
                           (args.raw_root / "net0-control-hil.py", "capture-tool.py"),
                           (args.raw_root / "net0-build-control.py", "build-tool.py"),
                           (args.backend / "Cargo.lock", "sta.Cargo.lock"),
                           (args.backend / "rust-toolchain.toml", "rust-toolchain.toml")):
        (HERE / target).write_bytes(source.read_bytes())
    (HERE / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    (HERE / "SHA256SUMS").write_text("".join(
        f"{sha(path.read_bytes())}  {path.name}\n" for path in sorted(HERE.iterdir())
        if path.is_file() and path.name != "SHA256SUMS"))
    print(json.dumps({"cases": [{k: c[k] for k in ("name", "requested", "recorded_passes", "udp_sent", "udp_received")} for c in cases]}))


if __name__ == "__main__":
    main()
