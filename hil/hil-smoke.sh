#!/usr/bin/env bash
# On-hardware (HIL) smoke test: flash each ws63-rs example to a real WS63 board
# and check its UART output, mirroring ws63-qemu/scripts/smoke-test.sh but on
# silicon. This validates what QEMU can't — real clocks/timing, real peripherals
# (notably the corrected 24 MHz TCXO timer + 160 MHz UART baud). REQUIRES a board
# + the flash env (see hil/flash.sh + hil/README.md).
#
# Usage:  PORT=/dev/ttyUSB0 LOADERBOOT=/path/loaderboot.bin ADDRESS=0x200000 hil/hil-smoke.sh
#
# Env: same as flash.sh (PORT/BAUD/LOADERBOOT/ADDRESS/HISIFLASH), plus:
#   SETTLE   seconds to read UART after each flash (default 4)
#   UART_BAUD  the examples' UART0 baud (default 115200, 8N1)
#   MONITOR  command that prints raw UART to stdout (default: raw read of $PORT)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SETTLE="${SETTLE:-4}"
UART_BAUD="${UART_BAUD:-115200}"
PORT="${PORT:?set PORT=/dev/ttyUSBx (the board UART0)}"
export HISIFLASH="${HISIFLASH:-hisiflash}"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
fail=0

# Read raw UART for $1 seconds to stdout. Override via $MONITOR for your adapter.
read_serial() {
    if [ -n "${MONITOR:-}" ]; then
        timeout "$1" bash -c "$MONITOR" 2>/dev/null || true
    else
        ( stty -F "$PORT" "$UART_BAUD" raw -echo 2>/dev/null; timeout "$1" cat "$PORT" ) 2>/dev/null || true
    fi
}

# check <example> <expected-egrep> <description>
check() {
    local ex="$1" pat="$2" desc="$3"
    echo "==> $ex: $desc"
    if ! "$HERE/flash.sh" "$ex" >/dev/null 2>"$TMP/flash.err"; then
        echo "    FAIL: flash failed"; tail -3 "$TMP/flash.err" | sed 's/^/      /'; fail=1; return
    fi
    read_serial "$SETTLE" > "$TMP/out"
    if grep -qE "$pat" "$TMP/out"; then
        echo "    PASS: '$pat' seen"
    else
        echo "    FAIL: '$pat' not seen. Got:"; tail -4 "$TMP/out" | sed 's/^/      /'; fail=1
    fi
}

echo "WS63 HIL smoke test on $PORT @ ${UART_BAUD} 8N1"
check uart_hello   "Hello from WS63"        "UART banner (validates the 160 MHz baud base)"
check timer_irq    "timer irq #|OK: timer"  "Timer IRQ delivery (validates the 24 MHz TCXO timer clock)"
check gpio_irq     "gpio irq #"             "GPIO IRQ delivery"
check reset_demo   "reset_reason=Software"  "software_reset + reset_reason"
check spi_loopback "SPI loopback OK"        "blocking SPI0 — SHORT MOSI<->MISO first!"
check i2c_scan     "scan done|no devices"   "I2C0 bus scan"

echo "--- blinky: GPIO0 toggle has no UART — verify with an LED / logic analyzer ---"
echo "--- semihost_selftest: needs a debugger (semihosting) — skipped on bare HIL ---"

[ "$fail" -eq 0 ] && echo "HIL SMOKE: PASS" || echo "HIL SMOKE: FAIL"
exit "$fail"
