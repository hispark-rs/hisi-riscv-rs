#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Bind native RX-stop attempts to source/ELF and whitelist public UART lines."""
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
SOURCE = "06d790bfe7d9016f7be44f42601b5afa567b9111"
ELF = "98b290173b4eea61fe6961f7d750127463e14e365f56b6c55f2ada29dbf50287"
FIELDS = ("installed", "requested", "entered", "returned", "post_returned", "mac_before",
          "mac_after", "descriptors_empty", "native_status", "fault", "expected_task", "current_task")
RX = re.compile(rb"RFDBG_NET0_RX_STOP" + rb" 0x([0-9a-f]{8})" * len(FIELDS))
OLD_RX = re.compile(rb"RFDBG_NET0_RX_STOP" + rb" 0x([0-9a-f]{8})" * 10)
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("tx", HERE.parent / "net0-host-tx-2026-09-09/collect-evidence.py")
tx = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tx)
prior = tx.prior


def sha(data):
    return hashlib.sha256(data).hexdigest()


def rx_contract(samples):
    if len(samples) != 2:
        return False
    first, last = samples
    return (first["installed"] == 1 and all(first[key] == 0 for key in FIELDS[1:5])
            and first["fault"] == 0 and all(last[key] == 1 for key in FIELDS[:5])
            and last["mac_after"] == 0 and last["descriptors_empty"] == 1
            and last["native_status"] == 0 and last["fault"] == 0
            and last.get("expected_task", 0) not in (0, 0xffffffff)
            and last.get("expected_task") == last.get("current_task"))


def collect(directory, requested, elf, classification):
    raw = (directory / "summary.json").read_bytes()
    original = json.loads(raw)
    if original["elf_sha256"] != elf:
        raise ValueError("Unexpected attempt ELF")
    result = {"name": directory.name, "requested": requested, "elf_sha256": elf,
              "classification": classification, "raw_summary_sha256": sha(raw),
              "original_source_label": original["source_commit"], "runs": []}
    if "flash" in original:
        result["flash"] = original["flash"]
    for row in original["runs"]:
        public = {}
        for role in ("target", "peer"):
            data = (directory / f"run-{row['index']:02}.{role}.uart.log").read_bytes()
            receipt = next(item for item in row["captures"] if item["role"] == role)
            if sha(data) != receipt["sha256"] or len(data) != receipt["bytes"]:
                raise ValueError("Capture bytes differ from original receipt")
            policy = prior.MARKERS if role == "target" else prior.PEER_MARKERS
            public[role] = [line.decode("ascii") for line in data.splitlines()
                            if policy.fullmatch(line) or (role == "target" and
                            (tx.TX.fullmatch(line) or RX.fullmatch(line) or OLD_RX.fullmatch(line)))]
            if role != "target":
                continue
            samples = []
            for line in data.splitlines():
                match = RX.fullmatch(line) or OLD_RX.fullmatch(line)
                if match:
                    samples.append(dict(zip(FIELDS, (int(v, 16) for v in match.groups()))))
            if samples != row["rx_stop"]["samples"]:
                raise ValueError("RX stop receipt differs from complete UART lines")
            tx_samples = [dict(zip(tx.FIELDS, (int(v, 16) for v in match.groups())))
                          for line in data.splitlines() if (match := tx.TX.fullmatch(line))]
            if tx_samples != row["host_tx"]["samples"]:
                raise ValueError("Host TX receipt differs from UART")
            payload_match = prior.match_one(prior.PAYLOAD, data)
            payload = None if payload_match is None else dict(zip(
                ("sent", "received", "bitmap", "invalid", "duplicate"),
                (int(v, 16) for v in payload_match.groups())))
            if payload != row["payload"]:
                raise ValueError("Payload receipt differs from UART")
            cleanup = prior.cleanup_observation(data, False)
            if cleanup["samples"] != row["cleanup"]["samples"]:
                raise ValueError("Cleanup receipt differs from UART")
            control = all(prior.match_one(rb"RFDBG_A5B_" + phase + rb"_OK elapsed_ms=0x[0-9a-f]{8}", data)
                          for phase in (b"CONNECT", b"DISCONNECT"))
            good = bool(control and cleanup["pass"] and tx.tx_contract(tx_samples)
                        and rx_contract(samples)
                        and payload == {"sent": 10, "received": 10, "bitmap": 1023, "invalid": 0, "duplicate": 0}
                        and all(marker in data.splitlines() for marker in (
                            b"RFDBG_NET0_PAYLOAD_OK", b"RFDBG_NET0_INITIAL_SESSION_CLOSED",
                            b"RFDBG_A5B_CONNECT_PROFILE_OK")))
            if good != row["pass"]:
                raise ValueError("Stored outcome differs from the complete contract")
        result["runs"].append(dict(row, public_markers=public))
    result["recorded_passes"] = sum(row["pass"] for row in result["runs"])
    result["udp_sent"] = sum((row.get("payload") or {}).get("sent", 0) for row in result["runs"])
    result["udp_received"] = sum((row.get("payload") or {}).get("received", 0) for row in result["runs"])
    if classification == "accepted" and (len(result["runs"]) != requested or result["recorded_passes"] != requested):
        raise ValueError("Incomplete/failed accepted matrix")
    if classification.startswith("rejected-") and result["recorded_passes"]:
        raise ValueError("A rejected candidate cannot be rewritten as accepted")
    if classification == "partial-matrix-failure" and not (
            0 < result["recorded_passes"] < len(result["runs"]) < requested):
        raise ValueError("Expected the preserved incomplete matrix with its failure")
    return result


class Tests(unittest.TestCase):
    def test_receipt_is_not_only_a_zero_status(self):
        first = dict(zip(FIELDS, (1, 0, 0, 0, 0, 255, 255, 255, 0, 0, 0, 0)))
        last = dict(zip(FIELDS, (1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 0x107, 0x107)))
        self.assertTrue(rx_contract([first, last]))
        for key, value in (("installed", 0), ("returned", 0), ("post_returned", 0),
                           ("mac_after", 1), ("descriptors_empty", 0), ("native_status", 1),
                           ("fault", 1), ("current_task", 7), ("current_task", 0x207), ("expected_task", 0)):
            self.assertFalse(rx_contract([first, dict(last, **{key: value})]))
        self.assertFalse(rx_contract([last]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw-root", type=Path)
    parser.add_argument("--backend", type=Path)
    parser.add_argument("--build-source", type=Path)
    parser.add_argument("--test", action="store_true")
    args = parser.parse_args()
    if args.test:
        if not unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests)).wasSuccessful():
            raise SystemExit(1)
    if args.raw_root is None:
        return
    root = args.raw_root
    if sha((args.build_source / "sta-rx-stop-v3.elf").read_bytes()) != ELF:
        raise ValueError("Accepted ELF changed")
    if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=args.backend, text=True).strip() != SOURCE:
        raise ValueError("Unexpected source commit")
    names = subprocess.check_output(["git", "ls-files", "-z", "src", "examples", "build.rs", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo"], cwd=args.backend).decode().split("\0")
    inputs = []
    for name in sorted(filter(None, names)):
        data = (args.backend / name).read_bytes()
        if data != (args.build_source / name).read_bytes():
            raise ValueError("Independent build input mismatch: " + name)
        inputs.append({"path": name, "sha256": sha(data)})
    cases = [collect(root / "net0-rx-stop-paired3-20260909", 3,
                     "1ca0e04bb184fb98eb98f27a1e3abfc38ef48624901c2c0dce29b06bcdf7d436", "rejected-rom-call"),
             collect(root / "net0-rx-stop-paired3-v2-20260909", 3,
                     "f0a18edd8b2bd64c0b1b43f0b9542b6df020553f2a240bd35483ec431b6b5501", "rejected-task-identity"),
             collect(root / "net0-rx-stop-paired3-v3-20260909", 3, ELF, "accepted"),
             collect(root / "net0-rx-stop-paired20-06d790b", 20, ELF, "partial-matrix-failure")]
    result = {"schema": "net0-rx-stop-hil/v1", "source_commit": SOURCE, "elf_sha256": ELF,
              "build_inputs": inputs, "cases": cases,
              "boundary": "Immediate terminal RX stop, host TX drain and one-shot traffic; not reusable DMA/host-RX fence or reconnect"}
    plan_root = root / "net0-rx-stop-paired3-v3-20260909/target/riscv32imfc-unknown-none-elf/release"
    result["image_sha256"] = sha((plan_root / "sta-rx-stop-v3.img").read_bytes())
    result["ap_elf_sha256"] = sha((root / "net0-control-hil-3914ff5/ap.elf").read_bytes())
    if result["ap_elf_sha256"] != "d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091":
        raise ValueError("Fixed AP changed")
    config = HERE.parents[3] / "examples/ws63/hil_wifi_config.rs"
    result["public_paired_config"] = {"path": "examples/ws63/hil_wifi_config.rs", "sha256": sha(config.read_bytes())}
    for source, target in ((plan_root / "sta-rx-stop-v3.plan.json", "flash-plan.json"),
                           (root / "net0-rx-stop-v3-native-link.json", "native-calls.json"),
                           (root / "net0-rx-stop-v3-storage.json", "storage.json"),
                           (root / "net0-control-hil.py", "capture-tool.py"),
                           (root / "net0-build-control.py", "build-tool.py"),
                           (args.backend / "Cargo.lock", "sta.Cargo.lock"),
                           (args.backend / "rust-toolchain.toml", "rust-toolchain.toml")):
        (HERE / target).write_bytes(source.read_bytes())
    (HERE / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    (HERE / "SHA256SUMS").write_text("".join(f"{sha(path.read_bytes())}  {path.name}\n" for path in sorted(HERE.iterdir())
        if path.is_file() and path.name != "SHA256SUMS"))
    print(json.dumps({"cases": [{k: c[k] for k in ("name", "requested", "recorded_passes", "udp_sent", "udp_received")} for c in cases]}))


if __name__ == "__main__":
    main()
