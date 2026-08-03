#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Capture one UART for a bounded duration without requiring a terminal."""

from __future__ import annotations

import argparse
import sys
import time

import serial


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", required=True)
    parser.add_argument("--baud", type=int, default=115_200)
    parser.add_argument("--duration", type=float, required=True)
    args = parser.parse_args()

    with serial.Serial(args.port, args.baud, timeout=0.2) as uart:
        uart.reset_input_buffer()
        deadline = time.monotonic() + args.duration
        while time.monotonic() < deadline:
            data = uart.read(4096)
            if data:
                sys.stdout.buffer.write(data)
                sys.stdout.buffer.flush()


if __name__ == "__main__":
    main()
