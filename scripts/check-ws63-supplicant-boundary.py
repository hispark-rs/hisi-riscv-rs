#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.9"
# dependencies = ["tomli>=2.0.1; python_version < '3.11'"]
# ///
"""Reject legacy vendor supplicant inputs from an upstream WS63 final link."""

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
    / "crates/chips/ws63/ws63-radio-sys/crates/hisi-rf-link/profiles/ws63-supplicant-boundary.toml"
)
LEGACY_SOURCE = ROOT / "chips/ws63/rf/src/wpa_compat.rs"
LIB_SOURCE = ROOT / "chips/ws63/rf/src/lib.rs"


class BoundaryError(RuntimeError):
    """The upstream/vendor supplicant boundary was violated."""


def load_profile(path: Path = PROFILE) -> dict[str, object]:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def source_exports(source: str) -> set[str]:
    functions = re.findall(
        r'#\[unsafe\(no_mangle\)\]\s*pub extern "C" fn\s+([A-Za-z0-9_]+)',
        source,
    )
    statics = re.findall(
        r"#\[unsafe\(no_mangle\)\]\s*pub static mut\s+([A-Za-z0-9_]+)",
        source,
    )
    return set(functions + statics)


def check_provider(profile: dict[str, object], source: str, lib_source: str) -> None:
    expected = set(profile["legacy_provider_symbols"])
    actual = source_exports(source)
    if actual != expected:
        raise BoundaryError(
            "legacy provider drift: "
            f"missing={sorted(expected - actual)}, unexpected={sorted(actual - expected)}"
        )
    if not re.search(
        r'#\[cfg\(feature = "wifi-personal"\)\]\s*mod\s+wpa_compat\s*;',
        lib_source,
    ):
        raise BoundaryError("wpa_compat is not isolated behind wifi-personal")


def archive_is_present(map_text: str, archive: str) -> bool:
    normalized = map_text.replace("\\", "/")
    return bool(
        re.search(
            rf"(?:^|[/\s]){re.escape(archive)}(?=\(|\s|$)",
            normalized,
            flags=re.MULTILINE,
        )
    )


def check_map(profile: dict[str, object], map_text: str) -> None:
    legacy = [
        archive
        for archive in profile["legacy_archives"]
        if archive_is_present(map_text, archive)
    ]
    if legacy:
        raise BoundaryError(f"legacy archives reached final link: {sorted(legacy)}")
    native = str(profile["native_archive"])
    markers = [str(marker) for marker in profile["native_object_markers"]]
    missing_markers = [marker for marker in markers if marker not in map_text]
    if not archive_is_present(map_text, native) and missing_markers:
        raise BoundaryError(
            "native supplicant provenance absent from final link: "
            f"archive={native}, missing_markers={missing_markers}"
        )


def check_symbols(profile: dict[str, object], defined: set[str]) -> None:
    legacy = defined & set(profile["legacy_provider_symbols"])
    if legacy:
        raise BoundaryError(
            f"legacy supplicant provider symbols reached final ELF: {sorted(legacy)}"
        )


def defined_symbols(elf: Path) -> set[str]:
    nm = (
        os.environ.get("NM")
        or shutil.which("riscv64-unknown-elf-nm")
        or shutil.which("llvm-nm")
    )
    if not nm:
        raise BoundaryError(
            "no RISC-V-capable nm found (set NM or install riscv64-unknown-elf-nm)"
        )
    result = subprocess.run(
        [nm, "--defined-only", "--format=posix", str(elf)],
        check=True,
        text=True,
        capture_output=True,
    )
    return {
        fields[0]
        for line in result.stdout.splitlines()
        if (fields := line.split())
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--elf", type=Path, help="final upstream-supplicant ELF")
    parser.add_argument("--map", type=Path, help="rust-lld final link map")
    args = parser.parse_args()

    profile = load_profile()
    check_provider(
        profile,
        LEGACY_SOURCE.read_text(encoding="utf-8"),
        LIB_SOURCE.read_text(encoding="utf-8"),
    )
    if args.map:
        if not args.map.is_file():
            raise BoundaryError(f"link map does not exist: {args.map}")
        check_map(profile, args.map.read_text(encoding="utf-8", errors="replace"))
    if args.elf:
        if not args.elf.is_file():
            raise BoundaryError(f"ELF does not exist: {args.elf}")
        check_symbols(profile, defined_symbols(args.elf))
    if bool(args.map) != bool(args.elf):
        raise BoundaryError("final-link verification requires both --map and --elf")

    suffix = ""
    if args.elf:
        suffix = f", final={args.elf}"
    print(
        "WS63 supplicant boundary OK: "
        f"native={profile['native_archive']}, "
        f"native_markers={len(profile['native_object_markers'])}, "
        f"legacy_archives={len(profile['legacy_archives'])}, "
        f"legacy_symbols={len(profile['legacy_provider_symbols'])}{suffix}"
    )


if __name__ == "__main__":
    try:
        main()
    except BoundaryError as error:
        print(f"WS63 supplicant boundary drift: {error}", file=sys.stderr)
        raise SystemExit(1) from error
