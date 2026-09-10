#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Bind direct-RX positive, layout-preserving negative, and restore evidence."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import unittest

HERE = Path(__file__).resolve().parent
SOURCE = "9afac3366e1a75cb7f734266a0d4a0322462716e"
ELF = "9f1e8d4341eb53c321b3793c391a94bb26b77856887a4c77a82e30732b109a1c"
NEGATIVE = "bc5b1cd5ad6995d34e8d09b3142440eba1e892ffc271adc3ba066779803d55f9"
AP = "d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091"
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("rx", HERE.parent / "net0-rx-stop-2026-09-09/collect-evidence.py")
rx = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rx)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def check_mutation(original, changed, manifest):
    if len(original) != len(changed):
        raise ValueError("Negative ELF must preserve layout")
    offset = manifest["file_offset"]
    if ([i for i, pair in enumerate(zip(original, changed)) if pair[0] != pair[1]]
            != [offset, offset + 1]
            or original[offset:offset + 2] != bytes.fromhex("01ed")
            or changed[offset:offset + 2] != bytes.fromhex("21a8")):
        raise ValueError("Only the reviewed two-byte branch may change")
    if manifest["input_sha256"] != sha(original) or manifest["output_sha256"] != sha(changed):
        raise ValueError("Mutation digest mismatch")


def check_rejection(case, flag):
    if flag != b"\x01" or len(case["runs"]) != 1 or case["recorded_passes"]:
        raise ValueError("Expected one intentional rejected run and a sticky fault")
    row = case["runs"][0]
    if row["payload"] is not None or any(row["markers"].values()):
        raise ValueError("Rejected run must not report successful connection or payload")


class Tests(unittest.TestCase):
    def test_extra_patch_or_changed_size_rejected(self):
        original, changed = b"a\x01\xedz", b"a\x21\xa8z"
        manifest = {"file_offset": 1, "input_sha256": sha(original), "output_sha256": sha(changed)}
        check_mutation(original, changed, manifest)
        for bad in (changed + b"x", b"b\x21\xa8z", original):
            with self.assertRaises(ValueError):
                check_mutation(original, bad, manifest)

    def test_timeout_alone_is_not_rejection_evidence(self):
        case = {"recorded_passes": 0, "runs": [{"payload": None, "markers": {"ok": False}}]}
        check_rejection(case, b"\x01")
        for flag in (b"", b"\x00", b"\x01\x00"):
            with self.assertRaises(ValueError):
                check_rejection(case, flag)
        case["runs"][0]["markers"]["ok"] = True
        with self.assertRaises(ValueError):
            check_rejection(case, b"\x01")


def main():
    if not __debug__:
        raise SystemExit("Do not disable evidence assertions")
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--raw-root", type=Path)
    p.add_argument("--backend", type=Path)
    p.add_argument("--build-source", type=Path)
    p.add_argument("--test", action="store_true")
    a = p.parse_args()
    if a.test and not unittest.TextTestRunner().run(
            unittest.defaultTestLoader.loadTestsFromTestCase(Tests)).wasSuccessful():
        raise SystemExit(1)
    if a.raw_root is None:
        return
    if a.backend is None or a.build_source is None:
        p.error("--backend and --build-source are required")
    original = (a.build_source / "sta.elf").read_bytes()
    changed = (a.build_source / "sta-force-queued-rx.elf").read_bytes()
    assert sha(original) == ELF and sha(changed) == NEGATIVE
    assert sha((a.raw_root / "net0-control-hil-3914ff5/ap.elf").read_bytes()) == AP
    manifest = json.loads((a.build_source / "queued-rx-mutation.json").read_bytes())
    assert manifest["source_commit"] == SOURCE and manifest["address"] == 0x29e282
    check_mutation(original, changed, manifest)
    names = subprocess.check_output([
        "git", "ls-tree", "-r", "--name-only", "-z", SOURCE, "--", "src", "examples",
        "build.rs", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo",
    ], cwd=a.backend).decode().split("\0")
    inputs = []
    for name in sorted(filter(None, names)):
        data = subprocess.check_output(["git", "show", SOURCE + ":" + name], cwd=a.backend)
        if data != (a.build_source / name).read_bytes():
            raise ValueError("Independent build differs from committed input: " + name)
        inputs.append({"path": name, "sha256": sha(data)})
    cases = []
    for directory, count, digest, classification in (
        ("net0-direct-rx3-9afac33", 3, ELF, "accepted"),
        ("net0-direct-rx-negative-9afac33", 1, NEGATIVE, "rejected-queued-rx"),
        ("net0-direct-rx-restored-9afac33", 1, ELF, "accepted"),
    ):
        path = a.raw_root / directory
        raw = json.loads((path / "summary.json").read_bytes())
        assert raw["source_commit"] == SOURCE + ("+queued-rx-fault" if digest == NEGATIVE else "")
        assert raw["reset_policy"] == {"kind": "paired-ap-then-sta", "peer_serial": "23121310"}
        case = rx.collect(path, count, digest, classification)
        flag = (path / "rejected-flag.bin").read_bytes()
        if digest == NEGATIVE:
            check_rejection(case, flag)
        elif flag != b"\x00":
            raise ValueError("Normal firmware has a sticky RX rejection")
        case["fault_readback"] = {"address": 0xa2dbc9, "value": flag[0], "sha256": sha(flag),
                                  "symbol": "__hisi_net0_queued_rx_rejected"}
        flash = path / "target/riscv32imfc-unknown-none-elf/release"
        stem = "sta-force-queued-rx" if digest == NEGATIVE else "sta"
        case["image_sha256"] = sha((flash / (stem + ".img")).read_bytes())
        (HERE / (classification + ("-restore" if "restored" in directory else "") + ".plan.json")).write_bytes(
            (flash / (stem + ".plan.json")).read_bytes())
        cases.append(case)
    config = HERE.parents[3] / "examples/ws63/hil_wifi_config.rs"
    result = {"schema": "net0-direct-rx-hil/v1", "source_commit": SOURCE,
              "elf_sha256": ELF, "negative_elf_sha256": NEGATIVE, "ap_elf_sha256": AP,
              "build_inputs": inputs, "cases": cases, "mutation": manifest,
              "public_paired_config": {"path": "examples/ws63/hil_wifi_config.rs", "sha256": sha(config.read_bytes())},
              "boundary": "Direct RX ownership and explicit queued-RX rejection only; not DMA fence, reinitialization or reconnect"}
    for source, target in (
        (a.build_source / "rx-mode.json", "rx-mode.json"),
        (a.build_source / "rx-stop.json", "rx-stop.json"),
        (a.build_source / "host-tx.json", "host-tx.json"),
        (a.build_source / "storage.json", "storage.json"),
        (a.build_source / "download-verification.json", "download-verification.json"),
        (a.raw_root / "net0-control-hil.py", "capture-tool.py"),
        (a.raw_root / "net0-build-control.py", "build-tool.py"),
        (a.raw_root / "net0-force-queued-rx.py", "mutate-queued-rx.py"),
        (a.build_source / "Cargo.lock", "sta.Cargo.lock"),
        (a.build_source / "rust-toolchain.toml", "rust-toolchain.toml"),
    ):
        (HERE / target).write_bytes(source.read_bytes())
    downloads = json.loads((HERE / "download-verification.json").read_bytes())
    assert downloads["source"] == SOURCE and downloads["successful_jobs"] == 23
    assert len(downloads["artifacts"]) == 3
    mode = json.loads((HERE / "rx-mode.json").read_bytes())
    assert mode["elf_sha256"] == ELF and mode["metadata"]["address"] == 0xa2dbc9
    (HERE / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    (HERE / "SHA256SUMS").write_text("".join(
        f"{sha(path.read_bytes())}  {path.name}\n" for path in sorted(HERE.iterdir())
        if path.is_file() and path.name != "SHA256SUMS"))
    print(json.dumps({"cases": [{k: case[k] for k in (
        "name", "classification", "recorded_passes", "udp_sent", "udp_received")} for case in cases]}))


if __name__ == "__main__":
    main()
