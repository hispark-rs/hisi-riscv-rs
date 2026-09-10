#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Preserve the handshake admission repair and the subsequent guarded rejection."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import unittest

HERE = Path(__file__).resolve().parent
SOURCE = "0f7486431335e94e4ec8bf6292f3e9a60d6e8a16"
ELF = "a234a29552117d828e2c2e0209a1d8ab1fcd9c0e1c6bdce82752a39643df2926"
AP = "d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091"
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("rx", HERE.parent / "net0-rx-stop-2026-09-09/collect-evidence.py")
rx = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rx)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    if not __debug__:
        raise SystemExit("Do not disable evidence assertions")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw-root", type=Path)
    parser.add_argument("--backend", type=Path)
    parser.add_argument("--build-source", type=Path)
    parser.add_argument("--test", action="store_true")
    args = parser.parse_args()
    if args.test and not unittest.TextTestRunner().run(
            unittest.defaultTestLoader.loadTestsFromTestCase(rx.Tests)).wasSuccessful():
        raise SystemExit(1)
    if args.raw_root is None:
        return
    if args.backend is None or args.build_source is None:
        parser.error("--backend and --build-source are required")
    assert sha((args.build_source / "sta.elf").read_bytes()) == ELF
    assert sha((args.raw_root / "net0-control-hil-3914ff5/ap.elf").read_bytes()) == AP
    names = subprocess.check_output([
        "git", "ls-tree", "-r", "--name-only", "-z", SOURCE, "--", "src", "examples",
        "build.rs", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo",
    ], cwd=args.backend).decode().split("\0")
    inputs = []
    for name in sorted(filter(None, names)):
        data = subprocess.check_output(["git", "show", SOURCE + ":" + name], cwd=args.backend)
        if data != (args.build_source / name).read_bytes():
            raise ValueError("Independent build differs from committed input: " + name)
        inputs.append({"path": name, "sha256": sha(data)})
    cases = []
    for count in (3, 20):
        directory = args.raw_root / f"net0-handshake-admission{count}-0f74864"
        original = json.loads((directory / "summary.json").read_bytes())
        assert original["source_commit"] == SOURCE
        assert original["reset_policy"] == {"kind": "paired-ap-then-sta", "peer_serial": "23121310"}
        rows = original["runs"]
        complete = len(rows) == count and all(row["pass"] for row in rows)
        if not complete and (not rows or rows[-1]["pass"] or not all(row["pass"] for row in rows[:-1])):
            raise ValueError("Expected a completed matrix or a preserved first terminal failure")
        assert [row["index"] for row in rows] == list(range(1, len(rows) + 1))
        cases.append(rx.collect(directory, count, ELF, "accepted" if complete else "partial-matrix-failure"))
    flash = args.raw_root / "net0-handshake-admission3-0f74864/target/riscv32imfc-unknown-none-elf/release"
    config = HERE.parents[3] / "examples/ws63/hil_wifi_config.rs"
    result = {
        "schema": "net0-handshake-admission-hil/v1", "source_commit": SOURCE,
        "elf_sha256": ELF, "image_sha256": sha((flash / "sta.img").read_bytes()),
        "ap_elf_sha256": AP, "build_inputs": inputs, "cases": cases,
        "public_paired_config": {"path": "examples/ws63/hil_wifi_config.rs", "sha256": sha(config.read_bytes())},
        "boundary": "Handshake TX admission only. The initial-session rejection is preserved; no native fence or reconnect claim",
    }
    for source, target in (
        (flash / "sta.plan.json", "flash-plan.json"),
        (args.build_source / "rx-stop.json", "rx-stop.json"),
        (args.build_source / "host-tx.json", "host-tx.json"),
        (args.build_source / "storage.json", "storage.json"),
        (args.build_source / "download-verification.json", "download-verification.json"),
        (args.raw_root / "net0-control-hil.py", "capture-tool.py"),
        (args.raw_root / "net0-build-control.py", "build-tool.py"),
        (args.build_source / "Cargo.lock", "sta.Cargo.lock"),
        (args.build_source / "rust-toolchain.toml", "rust-toolchain.toml"),
    ):
        (HERE / target).write_bytes(source.read_bytes())
    downloads = json.loads((HERE / "download-verification.json").read_bytes())
    assert downloads["source"] == SOURCE and downloads["successful_jobs"] == 23
    assert len(downloads["artifacts"]) == 3
    (HERE / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    (HERE / "SHA256SUMS").write_text("".join(
        f"{sha(path.read_bytes())}  {path.name}\n" for path in sorted(HERE.iterdir())
        if path.is_file() and path.name != "SHA256SUMS"))
    print(json.dumps({"cases": [{k: case[k] for k in (
        "name", "requested", "recorded_passes", "udp_sent", "udp_received")} for case in cases]}))


if __name__ == "__main__":
    main()
