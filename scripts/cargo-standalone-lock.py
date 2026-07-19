#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Generate or check a Rust crate Cargo.lock outside its parent workspace.

This is for publishable submodule crates that live inside the hisi-riscv-rs
monorepo. Running `cargo generate-lockfile` from a submodule path can still be
captured by the parent workspace and its [patch.crates-io] entries. This helper
copies the crate to a temporary standalone directory first, runs Cargo there,
and optionally copies the resulting Cargo.lock back.
"""

from __future__ import annotations

import argparse
import difflib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Check or update a crate Cargo.lock using standalone Cargo resolution."
    )
    parser.add_argument(
        "crate",
        nargs="?",
        default=".",
        help="Path to the crate directory containing Cargo.toml (default: cwd).",
    )
    parser.add_argument(
        "--update",
        action="store_true",
        help="Copy the standalone-generated Cargo.lock back into the crate.",
    )
    parser.add_argument(
        "--package",
        action="store_true",
        help="Also run a fully verified `cargo package --locked` in the standalone copy.",
    )
    parser.add_argument(
        "--cargo",
        default=os.environ.get("CARGO", "cargo"),
        help="Cargo executable to use (default: CARGO env or `cargo`).",
    )
    parser.add_argument(
        "--keep-temp",
        action="store_true",
        help="Keep the temporary standalone copy and print its path.",
    )
    return parser.parse_args()


def run(cmd: list[str], cwd: Path) -> None:
    print(f"+ {' '.join(cmd)}", flush=True)
    subprocess.run(cmd, cwd=cwd, check=True)


def copy_crate(src: Path, dst: Path) -> None:
    def ignore(_dir: str, names: list[str]) -> set[str]:
        ignored = {".git", "target"}
        return {name for name in names if name in ignored}

    shutil.copytree(src, dst, ignore=ignore, symlinks=True)


def read_text(path: Path) -> list[str]:
    if not path.exists():
        return []
    return path.read_text(encoding="utf-8").splitlines(keepends=True)


def print_diff(src_lock: Path, tmp_lock: Path) -> None:
    before = read_text(src_lock)
    after = read_text(tmp_lock)
    diff = difflib.unified_diff(
        before,
        after,
        fromfile=str(src_lock),
        tofile="standalone/Cargo.lock",
    )
    sys.stdout.writelines(diff)


def main() -> int:
    args = parse_args()
    crate = Path(args.crate).resolve()
    manifest = crate / "Cargo.toml"
    if not manifest.is_file():
        print(f"error: {manifest} does not exist", file=sys.stderr)
        return 2

    temp_root = Path(tempfile.mkdtemp(prefix="cargo-standalone-lock-"))
    standalone = temp_root / crate.name

    try:
        copy_crate(crate, standalone)
        run([args.cargo, "generate-lockfile"], standalone)
        tmp_lock = standalone / "Cargo.lock"
        src_lock = crate / "Cargo.lock"
        if not tmp_lock.is_file():
            print("error: cargo did not produce Cargo.lock", file=sys.stderr)
            return 1

        if args.package:
            run([args.cargo, "package", "--locked"], standalone)

        same = src_lock.is_file() and src_lock.read_bytes() == tmp_lock.read_bytes()
        if args.update:
            shutil.copy2(tmp_lock, src_lock)
            print(f"updated {src_lock}")
        elif not same:
            print("error: Cargo.lock differs under standalone resolution", file=sys.stderr)
            print_diff(src_lock, tmp_lock)
            return 1
        else:
            print(f"ok: {src_lock} matches standalone resolution")

        if args.keep_temp:
            print(f"kept temporary crate at {standalone}")
        return 0
    finally:
        if not args.keep_temp:
            shutil.rmtree(temp_root, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
