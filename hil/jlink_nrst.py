#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Shared J-Link hardware-reset support for WS63 HIL entry points."""

from __future__ import annotations

from contextlib import contextmanager
import os
from pathlib import Path
import subprocess
import tempfile
import time
from typing import Iterator


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


def interactive_jlink_argv(jlink: str, serial_number: str | None) -> list[str]:
    """Build an interactive Commander invocation for reset-line ownership."""
    argv = [jlink, "-NoGui", "1"]
    if serial_number is not None:
        if not serial_number.isascii() or not serial_number.isdecimal():
            raise ValueError("JLINK_SERIAL must contain decimal digits only")
        argv.extend(("-SelectEmuBySN", serial_number))
    return argv


@contextmanager
def held_nrst(jlink: str, serial_number: str | None = None) -> Iterator[None]:
    """Hold nRST for the context lifetime and release it on every exit path."""
    process = subprocess.Popen(
        interactive_jlink_argv(jlink, serial_number),
        stdin=subprocess.PIPE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.STDOUT,
        text=True,
    )
    assert process.stdin is not None
    try:
        process.stdin.write("SetRESET\nsleep 200\n")
        process.stdin.flush()
        time.sleep(0.4)
        if process.poll() is not None:
            raise RuntimeError(
                f"J-Link exited before nRST hold was established: {process.returncode}"
            )
        yield
    finally:
        if process.poll() is None:
            try:
                process.communicate("ClrRESET\nsleep 200\nq\n", timeout=5)
            except (BrokenPipeError, subprocess.TimeoutExpired):
                process.kill()
                process.wait(timeout=5)
                # A killed Commander may leave the probe driving nRST. A fresh
                # complete pulse provides an explicit target-independent release.
                pulse_nrst(jlink, serial_number)
        elif process.returncode != 0:
            # The reset owner disappeared unexpectedly; explicitly restore a
            # known released state before returning control to the caller.
            pulse_nrst(jlink, serial_number)
