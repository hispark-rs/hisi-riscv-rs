#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.9"
# dependencies = ["tomli>=2.0.1; python_version < '3.11'"]
# ///
"""Check the Rust provider and optional final ELF against the WS63 ABI profile."""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.9/3.10 through the uv single-script contract.
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[1]
PROFILE = (
    ROOT
    / "crates/chips/ws63/ws63-radio-sys/crates/hisi-rf-link/profiles/ws63-runtime-compat.toml"
)
SOURCE = ROOT / "chips/ws63/rf/src/ws63_runtime_compat.rs"
LEGACY_SOURCE = ROOT / "chips/ws63/rf/src/litos.rs"


def fail(message: str) -> None:
    print(f"WS63 runtime compatibility drift: {message}", file=sys.stderr)
    raise SystemExit(1)


parser = argparse.ArgumentParser()
parser.add_argument("--elf", type=Path, help="final upstream-supplicant ELF")
args = parser.parse_args()

if LEGACY_SOURCE.exists():
    fail("legacy litos.rs returned; use the bounded ws63_runtime_compat module")

profile = tomllib.loads(PROFILE.read_text())
patterns = [re.compile(pattern) for pattern in profile["namespace_patterns"]]
provided = {
    entry["name"] for entry in profile["symbols"] if entry["classification"] == "provided"
}
off_path = {
    entry["name"] for entry in profile["symbols"] if entry["classification"] == "off-path"
}

source = SOURCE.read_text()
exports = set(
    re.findall(
        r'#\[unsafe\(no_mangle\)\]\s*pub extern "C" fn\s+([A-Za-z0-9_]+)',
        source,
    )
)
if exports != provided:
    fail(
        "Rust provider mismatch: "
        f"missing={sorted(provided - exports)}, unexpected={sorted(exports - provided)}"
    )

lib_source = (ROOT / "chips/ws63/rf/src/lib.rs").read_text()
if re.search(r"\bmod\s+litos\b|\bpub\s+mod\s+litos\b", lib_source):
    fail("legacy litos module remains public or compiled")

if args.elf:
    if not args.elf.is_file():
        fail(f"ELF does not exist: {args.elf}")
    nm = os.environ.get("NM") or shutil.which("riscv64-unknown-elf-nm") or shutil.which("llvm-nm")
    if not nm:
        fail("no RISC-V-capable nm found (set NM or install riscv64-unknown-elf-nm)")
    result = subprocess.run(
        [nm, "--defined-only", "--format=posix", str(args.elf)],
        check=True,
        text=True,
        capture_output=True,
    )
    defined = {
        line.split()[0]
        for line in result.stdout.splitlines()
        if line.split() and any(pattern.search(line.split()[0]) for pattern in patterns)
    }
    resurrected = defined & off_path
    unexpected = defined - provided
    if resurrected:
        fail(f"off-path symbols became reachable: {sorted(resurrected)}")
    if unexpected:
        fail(f"unprofiled compatibility symbols in final ELF: {sorted(unexpected)}")
    print(f"WS63 final ELF compatibility surface OK: {sorted(defined)}")

print(f"WS63 runtime compatibility provider OK: {len(provided)} symbols")
