#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Shared J-Link hardware-reset support for WS63 HIL entry points."""

from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile


def jlink_argv(
    jlink: str, command_path: str | Path, serial_number: str | None
) -> list[str]:
    """Build a deterministic J-Link Commander invocation."""
    argv = [jlink, "-NoGui", "1"]
    if serial_number is not None:
        if not serial_number.isascii() or not serial_number.isdecimal():
            raise ValueError("JLINK_SERIAL must contain decimal digits only")
        argv.extend(("-SelectEmuBySN", serial_number))
    argv.extend(("-CommandFile", str(command_path)))
    return argv


def pulse_nrst(jlink: str, serial_number: str | None = None) -> None:
    """Pulse the target-independent hardware reset pin through J-Link."""
    command_file = tempfile.NamedTemporaryFile(
        mode="w",
        encoding="ascii",
        prefix="ws63-jlink-nrst-",
        suffix=".jlink",
        delete=False,
    )
    try:
        with command_file:
            command_file.write("SetRESET\n")
            command_file.write("sleep 200\n")
            command_file.write("ClrRESET\n")
            command_file.write("sleep 100\n")
            command_file.write("q\n")
        command_path = Path(command_file.name)
        argv = jlink_argv(jlink, command_path, serial_number)
    except BaseException:
        os.unlink(command_file.name)
        raise

    try:
        result = subprocess.run(
            argv,
            timeout=10,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            detail = (result.stderr or result.stdout).strip()
            raise RuntimeError(
                f"J-Link nRST failed with {result.returncode}: {detail}"
            )
    finally:
        command_path.unlink(missing_ok=True)
