#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Pulse the J-Link hardware reset pin, then optionally capture UART."""
import argparse
import os
import sys
import time

import serial

from jlink_nrst import jlink_argv, pulse_nrst

PORT = os.environ.get("PORT", "/dev/cu.wchusbserial120")
BAUD = int(os.environ.get("UART_BAUD", "115200"))
MONITOR = int(os.environ.get("MONITOR", "10"))
JLINK_SERIAL = os.environ.get("JLINK_SERIAL")


def nrst_jlink(serial_number: str | None):
    """Pulse hardware nRST through the probe's reset-pin commands.

    `r0` stopped driving the physical pin with J-Link Commander 9.52 even
    though it still returned success. SetRESET/ClrRESET is target-independent
    and was verified by observing the WS63 boot ROM UART banner.
    """
    pulse_nrst("JLinkExe", serial_number)


def reset_and_capture_uart(serial_number: str | None):
    # Open and drain UART before nRST so early boot ROM and firmware markers
    # cannot be discarded between the reset pulse and serial open.
    with serial.Serial(PORT, BAUD, timeout=0.3) as ser:
        ser.reset_input_buffer()
        nrst_jlink(serial_number)
        deadline = time.time() + MONITOR
        while time.time() < deadline:
            data = ser.read(4096)
            if data:
                sys.stdout.buffer.write(data)
                sys.stdout.flush()

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--reset-only", action="store_true")
    parser.add_argument("--jlink-serial", default=JLINK_SERIAL)
    args = parser.parse_args()
    if args.reset_only:
        nrst_jlink(args.jlink_serial)
    else:
        reset_and_capture_uart(args.jlink_serial)
