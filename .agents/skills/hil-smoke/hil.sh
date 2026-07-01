#!/usr/bin/env bash
# hil-smoke driver — the SILICON twin of qemu-smoke. Build a ws63-rs example for a
# chip, flash it to a real board via hisiflash, read UART, and assert the expected
# marker. Mirrors hil/hil-smoke.sh's markers; adds the chip→workspace build split,
# serial-port autodetect, LOADERBOOT autodiscovery, and a board-free --preflight.
#
#   bash hil.sh <chip> [example] [--preflight]
#     chip:    ws63 | bs21 | bs21e | bs22 | bs20
#     example: uart_hello | timer_irq | gpio_irq | reset_demo | spi_loopback | i2c_scan | …
#              (omit → the chip's full HIL smoke set)
#     --preflight: check the HIL environment (board/port/hisiflash/loaderboot/toolchain)
#                  WITHOUT building or flashing. Runnable with NO board attached.
#
# Env (same spirit as hil/flash.sh): PORT BAUD UART_BAUD LOADERBOOT ADDRESS HISIFLASH SETTLE
#   PORT       board UART0 serial port (autodetected if exactly one ttyUSB*/ttyACM*)
#   LOADERBOOT vendor loaderboot.bin (autodiscovered from fbb_ws63 / fbb_bs2x if unset)
#   ADDRESS    program flash offset (default 0x200000 — VERIFY vs the partition table)
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"            # .claude/skills/hil-smoke → repo root
TARGET=riscv32imfc-unknown-none-elf
SETTLE="${SETTLE:-4}"
UART_BAUD="${UART_BAUD:-115200}"
ADDRESS="${ADDRESS:-0x200000}"
HISIFLASH="${HISIFLASH:-hisiflash}"

CHIP="${1:-}"; shift || true
EXAMPLE=""; PREFLIGHT=0
for a in "$@"; do case "$a" in --preflight) PREFLIGHT=1 ;; *) EXAMPLE="$a" ;; esac; done
[ -n "$CHIP" ] || { echo "usage: hil.sh <chip> [example] [--preflight]   chip ∈ ws63|bs21|bs21e|bs22|bs20"; exit 2; }

# ── chip → build manifest / target dir / bin prefix / banner / loaderboot SDK ──
# bs21e/bs22 reuse the examples/bs21 (chip-bs21) binaries → banner says BS21.
case "$CHIP" in
    ws63)            MANIFEST="$REPO/Cargo.toml"; WS=""; TDIR="$REPO/target/$TARGET/release"; PFX="";      BANNER="WS63"; SDK="/root/fbb_ws63" ;;
    bs21|bs21e|bs22) MANIFEST="$REPO/examples/bs21/Cargo.toml"; WS="examples/bs21"; TDIR="$REPO/examples/bs21/target/$TARGET/release"; PFX="bs21_"; BANNER="BS21"; SDK="/root/fbb_bs2x" ;;
    bs20)            MANIFEST="$REPO/examples/bs20/Cargo.toml"; WS="examples/bs20"; TDIR="$REPO/examples/bs20/target/$TARGET/release"; PFX="bs20_"; BANNER="BS20"; SDK="/root/fbb_bs2x" ;;
    *) echo "FATAL: unknown chip '$CHIP' (ws63|bs21|bs21e|bs22|bs20)"; exit 2 ;;
esac
SET="uart_hello timer_irq gpio_irq reset_demo spi_loopback i2c_scan"

# ── env helpers (shared by preflight + the real run) ──────────────────────────
detect_port() {
    [ -n "${PORT:-}" ] && { echo "$PORT"; return; }
    local p; p=$(ls /dev/ttyUSB* /dev/ttyACM* 2>/dev/null)
    [ "$(echo "$p" | grep -c .)" = 1 ] && echo "$p"     # exactly one → use it
}
discover_loaderboot() {
    [ -n "${LOADERBOOT:-}" ] && { echo "$LOADERBOOT"; return; }
    [ -d "$SDK" ] || return
    find "$SDK" -iname '*loaderboot*.bin' 2>/dev/null | grep -vi sign | head -1
}
have_hisiflash() { command -v "$HISIFLASH" >/dev/null 2>&1; }
marker_for() {
    case "$1" in
        uart_hello)   echo "Hello from $BANNER" ;;
        timer_irq)    echo "timer irq #|OK: timer" ;;
        gpio_irq)     echo "gpio irq #" ;;
        reset_demo)   echo "reset_reason=Software" ;;
        spi_loopback) echo "SPI loopback OK" ;;
        i2c_scan)     echo "scan done|no devices" ;;
        *) echo "" ;;
    esac
}

# ── --preflight: report readiness, change nothing (works with no board) ───────
if [ "$PREFLIGHT" = 1 ]; then
    echo "════════ hil-smoke preflight: chip=$CHIP ════════"
    rdy=0
    if rustup toolchain list 2>/dev/null | grep -q hisi-riscv; then echo "  [ok]   hisi-riscv toolchain linked"; else echo "  [MISS] hisi-riscv toolchain — see run-ws63-rs skill"; rdy=1; fi
    if have_hisiflash; then echo "  [ok]   hisiflash: $(command -v $HISIFLASH)"; else echo "  [MISS] hisiflash — \`cargo install hisiflash-cli\` (or build /root/hisiflash)"; rdy=1; fi
    P="$(detect_port)"; if [ -n "$P" ]; then echo "  [ok]   serial port: $P"; else
        n=$(ls /dev/ttyUSB* /dev/ttyACM* 2>/dev/null | grep -c .)
        [ "$n" = 0 ] && echo "  [MISS] no board serial port (/dev/ttyUSB*|ttyACM*) — attach a board, or set PORT=" || echo "  [WARN] $n serial ports — set PORT= to pick one"
        rdy=1; fi
    L="$(discover_loaderboot)"; if [ -n "$L" ] && [ -f "$L" ]; then echo "  [ok]   loaderboot: $L"; else echo "  [MISS] no loaderboot under $SDK — build the SDK or set LOADERBOOT="; rdy=1; fi
    echo "  [info] ADDRESS=$ADDRESS (default — VERIFY against the board partition table)"
    [ "$CHIP" != ws63 ] && echo "  [warn] BS2X real-hardware HIL is UNVERIFIED (see hil/README.md); QEMU path is solid"
    echo "  → $([ $rdy = 0 ] && echo READY || echo NOT-READY) (preflight changed nothing)"
    exit $rdy
fi

# ── real run: resolve env, build, then flash + read + assert ──────────────────
PORT="$(detect_port)";        [ -n "$PORT" ]        || { echo "FATAL: no serial port — set PORT=/dev/ttyUSBx"; exit 2; }
LOADERBOOT="$(discover_loaderboot)"; [ -n "$LOADERBOOT" ] && [ -f "$LOADERBOOT" ] || { echo "FATAL: no LOADERBOOT — set LOADERBOOT=<vendor loaderboot.bin>"; exit 2; }
have_hisiflash || { echo "FATAL: hisiflash not found — cargo install hisiflash-cli"; exit 2; }
export PORT LOADERBOOT ADDRESS HISIFLASH UART_BAUD SETTLE WS63_RS="$REPO"

build() {  # ws63 via -p in root ws; bs2x via the isolated-workspace manifest
    if [ -z "$WS" ]; then
        local pf=""; for c in $1; do pf="$pf -p $c"; done
        echo "==> cargo build --release$pf"; ( cd "$REPO" && cargo build --release $pf )
    else
        echo "==> cargo build --manifest-path $WS/Cargo.toml --release"; ( cd "$REPO" && cargo build --manifest-path "$MANIFEST" --release )
    fi
}
read_serial() {  # raw UART for $1 s; override with $MONITOR for your adapter
    if [ -n "${MONITOR:-}" ]; then timeout "$1" bash -c "$MONITOR" 2>/dev/null || true
    else ( stty -F "$PORT" "$UART_BAUD" raw -echo 2>/dev/null; timeout "$1" cat "$PORT" ) 2>/dev/null || true; fi
}
flash_one() {  # build .bin via hil/flash.sh; HIL_CONFIRM marks the deliberate write (see flash-guard hook)
    HIL_CONFIRM=1 "$REPO/hil/flash.sh" "$1"
}
do_check() {   # build+flash+read+assert one example; echoes PASS/FAIL
    local ex="$1" pat; pat="$(marker_for "$ex")"
    build "$ex" >/dev/null 2>&1 || { echo "  FAIL $ex — build"; return 1; }
    local bin="$TDIR/${PFX}${ex}"; [ -f "$bin" ] || bin="$TDIR/${ex}"
    echo "==> $ex: flash + read UART ${SETTLE}s (marker: ${pat:-none})"
    if ! flash_one "$bin" >/dev/null 2>&1 && ! flash_one "$ex" >/dev/null 2>&1; then echo "  FAIL $ex — flash"; return 1; fi
    local out; out="$(read_serial "$SETTLE")"
    if [ -z "$pat" ]; then echo "  INFO $ex — no UART marker (e.g. blinky = LED/logic-analyzer); first lines:"; echo "$out" | head -3 | sed 's/^/    /'; return 0; fi
    if echo "$out" | grep -qE "$pat"; then echo "  PASS $ex — '$pat' seen"; return 0
    else echo "  FAIL $ex — '$pat' not seen. Got:"; echo "$out" | tail -4 | sed 's/^/    /'; return 1; fi
}

echo "════════ hil-smoke: chip=$CHIP  port=$PORT  loaderboot=$(basename "$LOADERBOOT")  addr=$ADDRESS ════════"

# single example
if [ -n "$EXAMPLE" ]; then do_check "$EXAMPLE"; exit $?; fi

# full suite — ws63 delegates to the in-tree hil/hil-smoke.sh (source of truth);
# bs2x runs the chip-aware checks inline (hil-smoke.sh is WS63-only).
build "$SET" || { echo "FATAL: build failed"; exit 1; }
if [ "$CHIP" = ws63 ] && [ -x "$REPO/hil/hil-smoke.sh" ]; then
    echo "==> delegating to hil/hil-smoke.sh (WS63 source of truth)"
    HIL_CONFIRM=1 bash "$REPO/hil/hil-smoke.sh"; exit $?
fi
fail=0; for ex in $SET; do do_check "$ex" || fail=1; done
echo "════════ HIL SMOKE ($CHIP): $([ $fail = 0 ] && echo PASS || echo FAIL) ════════"; exit $fail
