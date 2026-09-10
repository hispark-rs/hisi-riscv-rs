#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Bind the RX deadline regression preflight to committed inputs and raw UART."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
SOURCE = "8a6908c193fc6756d5f68b9e919c09903189a5e2"
ELF = "4229b1c78a55f449a88ad6a2d18052e912155287a47aefdbe3bc4c44a22c99ab"
AP = "d54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091"
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location(
    "rebuild", HERE.parent / "net0-rx-rebuild-2026-09-10/collect-evidence.py")
rebuild = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rebuild)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--raw-root", type=Path, required=True)
    p.add_argument("--backend", type=Path, required=True)
    p.add_argument("--build-source", type=Path, required=True)
    a = p.parse_args()
    if not __debug__:
        p.error("Evidence assertions must remain enabled")
    assert sha((a.build_source / "sta.elf").read_bytes()) == ELF
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
    directory = a.raw_root / "net0-rx-deadline3-8a6908c"
    raw = json.loads((directory / "summary.json").read_bytes())
    assert raw["source_commit"] == SOURCE
    assert raw["reset_policy"] == {"kind": "paired-ap-then-sta", "peer_serial": "23121310"}
    complete = len(raw["runs"]) == 3 and all(row["pass"] for row in raw["runs"])
    case = rebuild.stop.collect(directory, 3, ELF, "accepted" if complete else "preserved-preflight-failure")
    for row in case["runs"]:
        data = (directory / f"run-{row['index']:02}.target.uart.log").read_bytes()
        lines = [line for line in data.splitlines() if rebuild.RX.fullmatch(line)]
        samples = [dict(zip(rebuild.FIELDS, (int(value, 16) for value in rebuild.RX.fullmatch(line).groups())))
                   for line in lines]
        assert samples == row["rx_rebuild"]["samples"]
        assert rebuild.valid(samples) == row["rx_rebuild"]["pass"]
        if row["pass"]:
            assert rebuild.valid(samples)
        row["public_markers"]["target"].extend(line.decode("ascii") for line in lines)
    assert case["flash"]["verify_requested"] and case["flash"]["exit_code"] == 0
    assert case["flash"]["speed_khz"] == 3000
    flash = directory / "target/riscv32imfc-unknown-none-elf/release"
    case["image_sha256"] = sha((flash / "sta.img").read_bytes())
    for source, target in (
        (flash / "sta.plan.json", "flash-plan.json"),
        (a.build_source / "rx-stop.json", "rx-stop.json"),
        (a.build_source / "rx-mode.json", "rx-mode.json"),
        (a.build_source / "host-tx.json", "host-tx.json"),
        (a.build_source / "storage.json", "storage.json"),
        (a.build_source / "download-verification.json", "download-verification.json"),
        (a.build_source / "Cargo.lock", "sta.Cargo.lock"),
        (a.build_source / "rust-toolchain.toml", "rust-toolchain.toml"),
    ):
        (HERE / target).write_bytes(source.read_bytes())
    for name in ("rx-stop", "rx-mode", "host-tx", "storage"):
        report = json.loads((HERE / (name + ".json")).read_bytes())
        assert report["elf_sha256"] == ELF and report["status"] == "pass"
    rx = json.loads((HERE / "rx-stop.json").read_bytes())
    assert rx["schema"] == "net0-rx-stop-link/v3" and rx["metadata"]["bytes"] == 80
    assert rx["rejected_call_and_address_mutations"] == 25
    downloads = json.loads((HERE / "download-verification.json").read_bytes())
    assert downloads["source"] == SOURCE and downloads["successful_jobs"] == 23
    capture = HERE.parent / "net0-rx-rebuild-2026-09-10/capture-tool.py"
    build = HERE.parent / "net0-rx-rebuild-2026-09-10/build-tool.py"
    assert capture.read_bytes() == (a.raw_root / "net0-rebuild-hil.py").read_bytes()
    assert build.read_bytes() == (a.raw_root / "net0-build-control.py").read_bytes()
    result = {
        "schema": "net0-rx-deadline-hil/v1", "source_commit": SOURCE,
        "elf_sha256": ELF, "ap_elf_sha256": AP, "build_inputs": inputs, "cases": [case],
        "public_paired_config": {"path": "examples/ws63/hil_wifi_config.rs",
            "sha256": sha((HERE.parents[3] / "examples/ws63/hil_wifi_config.rs").read_bytes())},
        "tools": [{"path": "../net0-rx-rebuild-2026-09-10/" + path.name,
                   "sha256": sha(path.read_bytes())} for path in (capture, build)],
        "boundary": "Normal terminal preflight only; host deadline negatives do not prove bounded native C or DMA quiescence",
    }
    (HERE / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    (HERE / "SHA256SUMS").write_text("".join(
        f"{sha(path.read_bytes())}  {path.name}\n" for path in sorted(HERE.iterdir())
        if path.is_file() and path.name != "SHA256SUMS"))
    print(json.dumps({key: case[key] for key in
                      ("classification", "recorded_passes", "udp_sent", "udp_received")}))


if __name__ == "__main__":
    main()
