#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Prepare/reverify a firmware release using the hisi-fwpkg format owner."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
INPUTS = ("blinky.elf", "blinky.img", "blinky.plan.json", "Cargo.lock", "rust-toolchain.toml")
ASSETS = (*INPUTS, "release-manifest.json", "SHA256SUMS")


def run(*command: str, cwd: Path = ROOT) -> str:
    return subprocess.check_output(command, cwd=cwd, text=True).strip()


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def plan(elf: Path, output: Path) -> dict:
    return json.loads(run("hisi-fwpkg", "plan", str(elf), "--chip", "ws63", "--image-output", str(output)))


def verify_checksums(directory: Path, expected: set[str] | None = None) -> dict[str, str]:
    hashes = {}
    for line in (directory / "SHA256SUMS").read_text().splitlines():
        checksum, name = line.split(maxsplit=1)
        name = name.removeprefix("*")
        if (not name or name in hashes or Path(name).name != name or "\\" in name
                or name in (".", "..", "SHA256SUMS")):
            raise ValueError("unsafe or duplicate checksum filename")
        if len(checksum) != 64 or sha(directory / name) != checksum:
            raise ValueError(f"checksum mismatch: {name}")
        hashes[name] = checksum
    actual = {path.name for path in directory.iterdir() if path.is_file()} - {"SHA256SUMS"}
    if not hashes or set(hashes) != actual or (expected is not None and actual != expected):
        raise ValueError("checksum manifest does not cover the exact asset set")
    return hashes


def verify(directory: Path, expected_source: str, expected_version: str) -> dict:
    hashes = verify_checksums(directory, set(ASSETS) - {"SHA256SUMS"})
    manifest = json.loads((directory / "release-manifest.json").read_text())
    if (manifest.get("schema") != 1 or manifest.get("source_commit") != expected_source
            or manifest.get("version") != expected_version):
        raise ValueError("release source/version mismatch")
    if manifest.get("artifacts") != {name: hashes[name] for name in INPUTS}:
        raise ValueError("release manifest artifacts mismatch")
    if not manifest.get("submodules") or not manifest.get("source_ci"):
        raise ValueError("missing release train/source CI identity")
    for name in ("rustc", "cargo", "hisi-fwpkg"):
        if not manifest.get("tools", {}).get(name):
            raise ValueError(f"missing tool identity: {name}")
    if run("hisi-fwpkg", "--version") != manifest["tools"]["hisi-fwpkg"]:
        raise ValueError("install the exact hisi-fwpkg version from release-manifest.json")
    declared = json.loads((directory / "blinky.plan.json").read_text())
    spec = importlib.util.spec_from_file_location("flash_plan_check", ROOT / "scripts/check-flash-plan.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    image = (directory / "blinky.img").read_bytes()
    module.validate_flash_plan(declared, image)
    module.validate_image_semantics(declared, image)
    with tempfile.TemporaryDirectory(prefix="release-reverify-") as temp:
        rebuilt = Path(temp) / "image.img"
        actual = plan(directory / "blinky.elf", rebuilt)
        if actual != declared or rebuilt.read_bytes() != image:
            raise ValueError("release image/plan cannot be reproduced from the released ELF")
    return {"schema": 1, "status": "pass", "source_commit": expected_source,
            "version": expected_version, "downloaded_asset_sha256": hashes,
            "scope": "byte integrity, train identity, ELF-to-image equivalence; no new HIL claim"}


def prepare(args: argparse.Namespace) -> None:
    source = run("git", "rev-parse", "HEAD")
    run("git", "diff", "--exit-code", "HEAD")
    args.directory.mkdir(parents=True, exist_ok=True)
    if any(args.directory.iterdir()):
        raise ValueError("release candidate directory must be empty")
    shutil.copy2(args.elf, args.directory / "blinky.elf")
    for name in ("Cargo.lock", "rust-toolchain.toml"):
        shutil.copy2(ROOT / name, args.directory / name)
    write_json(args.directory / "blinky.plan.json", plan(args.elf, args.directory / "blinky.img"))
    submodules = []
    for line in run("git", "submodule", "status", "--recursive").splitlines():
        if line[0] in "-+U":
            raise ValueError("release submodules must be initialized at their pinned commits")
        commit, path, *_ = line.strip().split()
        submodules.append({"path": path, "commit": commit})
    manifest = {"schema": 1, "version": args.version, "source_commit": source,
                "source_ci": args.source_ci, "submodules": submodules,
                "build_command": "cargo build --locked -Zbuild-std=core,alloc --release -p blinky",
                "tools": {tool: run(tool, "-vV" if tool in ("cargo", "rustc") else "--version")
                          for tool in ("cargo", "rustc", "hisi-fwpkg")},
                "artifacts": {name: sha(args.directory / name) for name in INPUTS}}
    write_json(args.directory / "release-manifest.json", manifest)
    (args.directory / "SHA256SUMS").write_text("".join(
        f"{sha(args.directory / name)}  {name}\n" for name in ASSETS if name != "SHA256SUMS"))
    verify(args.directory, source, args.version)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("prepare", "verify"))
    parser.add_argument("directory", type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--elf", type=Path)
    parser.add_argument("--source-ci")
    parser.add_argument("--expected-source")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    args.directory = args.directory.resolve()
    if args.command == "prepare":
        if args.elf is None or not args.source_ci:
            parser.error("prepare requires --elf and --source-ci")
        args.elf = args.elf.resolve()
        prepare(args)
    else:
        if not args.expected_source:
            parser.error("verify requires --expected-source")
        result = verify(args.directory, args.expected_source, args.version)
        if args.report:
            write_json(args.report, result)
        print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
