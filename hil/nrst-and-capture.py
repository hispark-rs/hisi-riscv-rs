#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Pulse JLink hardware nRST (r0), then capture UART for MONITOR seconds."""
import sys, os, time, subprocess
import serial

PORT = os.environ.get("PORT", "/dev/cu.wchusbserial120")
BAUD = int(os.environ.get("UART_BAUD", "115200"))
MONITOR = int(os.environ.get("MONITOR", "10"))
JLINK_CMD = "/tmp/_jlink_nrst_cmd.txt"

def nrst_jlink():
    """Pulse hardware nRST via JLinkExe.  r0 resets the target without
    requiring a debug connection (unlike `r` which needs the device to be
    known)."""
    with open(JLINK_CMD, "w") as f:
        f.write("device WS63\n")
        f.write("si 1\n")
        f.write("speed 4000\n")
        f.write("r0\n")
        f.write("q\n")
    try:
        subprocess.run(
            ["JLinkExe", "-NoGui", "1", "-CommandFile", JLINK_CMD],
            timeout=10, capture_output=True
        )
    except Exception:
        pass
    finally:
        if os.path.exists(JLINK_CMD):
            os.unlink(JLINK_CMD)

def capture_uart():
    ser = serial.Serial(PORT, BAUD, timeout=0.3)
    ser.reset_input_buffer()
    # Small delay: nRST pulse has been sent; wait for boot ROM to start talking.
    time.sleep(0.5)
    deadline = time.time() + MONITOR
    while time.time() < deadline:
        b = ser.read(4096)
        if b:
            sys.stdout.buffer.write(b)
            sys.stdout.flush()
    ser.close()

if __name__ == "__main__":
    nrst_jlink()
    capture_uart()
