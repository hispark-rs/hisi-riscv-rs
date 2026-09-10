#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Recompute the closed-admission descriptor round-trip from original UART."""
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
SOURCE = "5e7b179feadf9600aebc356f40d49748dd1ed5cd"
ELF = "f60cf55a4cadc9c3bce563ac5297db2f4989f13b89c6296802338e64f0ebadc1"
NEGATIVE = "9d9528b04ecd789566ef62431282d792db39cd363ad7e1aeda7d22d29b3b58ce"
AP = "d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091"
FIELDS = ("attempted", "cleanup_attempted", "expected_normal", "expected_high", "expected_small",
          "actual_normal", "actual_high", "actual_small", "cleaned_normal", "cleaned_high", "cleaned_small",
          "mac_after_init", "mac_after_cleanup", "init_status", "cleanup_status", "fault")
RX = re.compile(rb"RFDBG_NET0_RX_REBUILD" + rb" 0x([0-9a-f]{8})" * len(FIELDS))
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("stop", HERE.parent / "net0-rx-stop-2026-09-09/collect-evidence.py")
stop = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stop)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def initial(sample):
    return sample == dict.fromkeys(FIELDS, 0) | {"mac_after_init": 255, "mac_after_cleanup": 255}


def valid(samples):
    if len(samples) != 2:
        return False
    first, last = samples
    return (initial(first)
            and last["attempted"] == last["cleanup_attempted"] == 1
            and sum(last[key] for key in FIELDS[2:5]) > 0
            and all(last["expected_" + name] == last["actual_" + name]
                    and last["cleaned_" + name] == 0 for name in ("normal", "high", "small"))
            and all(last[key] == 0 for key in FIELDS[11:]))


def check_negative(samples):
    if len(samples) != 2:
        raise ValueError("Negative test requires before/after receipts")
    last = samples[-1]
    if (not initial(samples[0]) or last["attempted"] != 1 or last["cleanup_attempted"] != 1
            or [last[key] for key in FIELDS[2:5]] != [4, 4, 8]
            or any(last[key] != 0 for key in FIELDS[5:15])
            or last["fault"] != (-0x1026 & 0xffffffff)):
        raise ValueError("Expected zero-allocation rejection with successful cleanup")


def check_mutation(original, changed, manifest):
    offset = manifest["file_offset"]
    if (len(original) != len(changed) or original == changed
            or original[:offset] != changed[:offset] or original[offset + 8:] != changed[offset + 8:]
            or original[offset:offset + 8].hex() != manifest["old_bytes"]
            or changed[offset:offset + 8].hex() != "1305000013000000"
            or sha(original) != manifest["input_sha256"] or sha(changed) != manifest["output_sha256"]):
        raise ValueError("Negative ELF differs outside the reviewed zero-return call")


class Tests(unittest.TestCase):
    def test_exact_queues_and_cleanup_not_only_zero_status(self):
        first = dict.fromkeys(FIELDS, 0) | {"mac_after_init": 255, "mac_after_cleanup": 255}
        last = dict(zip(FIELDS, (1, 1, 4, 4, 8, 4, 4, 8, 0, 0, 0, 0, 0, 0, 0, 0)))
        self.assertTrue(valid([first, last]))
        for name, value in (("actual_normal", 3), ("actual_high", 0), ("actual_small", 4),
                            ("cleaned_high", 1), ("mac_after_init", 1), ("mac_after_cleanup", 1),
                            ("init_status", 1), ("cleanup_status", 1), ("fault", 1),
                            ("attempted", 0), ("cleanup_attempted", 0)):
            self.assertFalse(valid([first, dict(last, **{name: value})]))
        self.assertFalse(valid([last]))
        empty = dict(last, **{name: 0 for name in FIELDS[2:8]})
        self.assertFalse(valid([first, empty]))

    def test_negative_requires_specific_fault_and_cleaned_lists(self):
        samples = [dict.fromkeys(FIELDS, 0) | {"mac_after_init": 255, "mac_after_cleanup": 255}, dict(zip(FIELDS,
            (1, 1, 4, 4, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -0x1026 & 0xffffffff)))]
        check_negative(samples)
        for field, value in (("fault", 0), ("cleaned_normal", 1), ("cleanup_status", 1), ("actual_high", 4)):
            with self.assertRaises(ValueError):
                check_negative([samples[0], dict(samples[1], **{field: value})])


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--raw-root", type=Path)
    p.add_argument("--backend", type=Path)
    p.add_argument("--build-source", type=Path)
    p.add_argument("--test", action="store_true")
    a = p.parse_args()
    if not __debug__:
        p.error("Evidence assertions must remain enabled")
    if a.test and not unittest.TextTestRunner().run(
            unittest.defaultTestLoader.loadTestsFromTestCase(Tests)).wasSuccessful():
        raise SystemExit(1)
    if a.raw_root is None:
        return
    if a.backend is None or a.build_source is None:
        p.error("--backend and --build-source are required")
    normal = (a.build_source / "sta.elf").read_bytes()
    negative = (a.build_source / "sta-skip-init.elf").read_bytes()
    assert sha(normal) == ELF and sha(negative) == NEGATIVE
    mutation = json.loads((a.build_source / "skip-init-mutation.json").read_bytes())
    check_mutation(normal, negative, mutation)
    assert sha((a.raw_root / "net0-control-hil-3914ff5/ap.elf").read_bytes()) == AP
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
    for name, count, digest, classification in (
        ("net0-rx-rebuild3-5e7b179", 3, ELF, "accepted"),
        ("net0-rx-rebuild-negative-5e7b179", 1, NEGATIVE, "rejected-zero-allocation"),
        ("net0-rx-rebuild-restored-5e7b179", 1, ELF, "accepted"),
    ):
        directory = a.raw_root / name
        raw = json.loads((directory / "summary.json").read_bytes())
        assert raw["source_commit"] == SOURCE + ("+skip-init" if digest == NEGATIVE else "")
        assert raw["reset_policy"] == {"kind": "paired-ap-then-sta", "peer_serial": "23121310"}
        case = stop.collect(directory, count, digest, classification)
        for row in case["runs"]:
            data = (directory / f"run-{row['index']:02}.target.uart.log").read_bytes()
            lines = [line for line in data.splitlines() if RX.fullmatch(line)]
            samples = [dict(zip(FIELDS, (int(value, 16) for value in RX.fullmatch(line).groups()))) for line in lines]
            if samples != row["rx_rebuild"]["samples"]:
                raise ValueError("Descriptor receipt differs from UART")
            if row["rx_rebuild"]["pass"] != valid(samples):
                raise ValueError("Stored rebuild outcome differs from recomputed contract")
            if digest == NEGATIVE:
                check_negative(samples)
                if row["pass"] or b"RFDBG_A5B_DISCONNECT_OK" in data or b"RFDBG_NET0_INITIAL_SESSION_CLOSED" in data:
                    raise ValueError("Negative test must not report successful terminal operation")
            elif not valid(samples):
                raise ValueError("Descriptor round-trip failed")
            row["public_markers"]["target"].extend(line.decode("ascii") for line in lines)
        assert case["flash"]["verify_requested"] and case["flash"]["exit_code"] == 0
        assert case["flash"]["speed_khz"] == 3000
        flash = directory / "target/riscv32imfc-unknown-none-elf/release"
        stem = "sta-skip-init" if digest == NEGATIVE else "sta"
        case["image_sha256"] = sha((flash / (stem + ".img")).read_bytes())
        (HERE / (name + ".plan.json")).write_bytes((flash / (stem + ".plan.json")).read_bytes())
        cases.append(case)
    result = {"schema": "net0-rx-rebuild-hil/v1", "source_commit": SOURCE, "elf_sha256": ELF,
              "ap_elf_sha256": AP, "negative_elf_sha256": NEGATIVE, "mutation": mutation,
              "build_inputs": inputs, "cases": cases,
              "public_paired_config": {"path": "examples/ws63/hil_wifi_config.rs",
                  "sha256": sha((HERE.parents[3] / "examples/ws63/hil_wifi_config.rs").read_bytes())},
              "boundary": "Closed-admission native allocation round-trip only; no DMA fence, repeated allocation or reconnect claim"}
    for source, target in (
        (a.raw_root / "net0-skip-rx-init.py", "mutate-skip-init.py"),
        (a.build_source / "rx-stop.json", "rx-stop.json"),
        (a.build_source / "rx-mode.json", "rx-mode.json"),
        (a.build_source / "host-tx.json", "host-tx.json"),
        (a.build_source / "storage.json", "storage.json"),
        (a.build_source / "download-verification.json", "download-verification.json"),
        (a.raw_root / "net0-rebuild-hil.py", "capture-tool.py"),
        (a.raw_root / "net0-build-control.py", "build-tool.py"),
        (a.build_source / "Cargo.lock", "sta.Cargo.lock"),
        (a.build_source / "rust-toolchain.toml", "rust-toolchain.toml"),
    ):
        (HERE / target).write_bytes(source.read_bytes())
    downloads = json.loads((HERE / "download-verification.json").read_bytes())
    assert downloads["source"] == SOURCE and downloads["successful_jobs"] == 23
    assert len(downloads["artifacts"]) == 3
    for name in ("rx-stop", "rx-mode", "host-tx", "storage"):
        report = json.loads((HERE / (name + ".json")).read_bytes())
        assert report["elf_sha256"] == ELF and report["status"] == "pass"
    (HERE / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    (HERE / "SHA256SUMS").write_text("".join(
        f"{sha(path.read_bytes())}  {path.name}\n" for path in sorted(HERE.iterdir())
        if path.is_file() and path.name != "SHA256SUMS"))
    print(json.dumps({"cases": [{k: c[k] for k in ("name", "classification", "recorded_passes", "udp_sent", "udp_received")} for c in cases]}))


if __name__ == "__main__":
    main()
