#!/usr/bin/env bash
# qemu-vs-hil parity — run the SAME example through both engines and diff the markers.
# This is the reason the HIL layer exists: the QEMU model's credibility rests on
# matching silicon. Each row is checked against ONE shared marker table (the HIL
# truth); timing-sensitive rows (baud, timer period) are flagged — they are exactly
# what QEMU cannot prove (see hil/README.md bring-up steps 3-4).
#
#   bash parity.sh <chip> [example]
#     chip:    ws63 | bs21 | bs21e | bs22 | bs20
#     example: omit → the common UART-marker set
#
# With no board, the HIL column reads "n/a" (preflight NOT-READY) and only the QEMU
# column is filled — still useful to confirm the QEMU side before a board arrives.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILLS="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
TARGET=riscv32imfc-unknown-none-elf
TIMEOUT="${TIMEOUT:-6}"

CHIP="${1:-}"; EXAMPLE="${2:-}"
[ -n "$CHIP" ] || { echo "usage: parity.sh <chip> [example]   chip ∈ ws63|bs21|bs21e|bs22|bs20"; exit 2; }

case "$CHIP" in
    ws63)            MANIFEST="$REPO/Cargo.toml"; WS=""; TDIR="$REPO/target/$TARGET/release"; PFX=""; BANNER="WS63" ;;
    bs21|bs21e|bs22) MANIFEST="$REPO/examples/bs21/Cargo.toml"; WS="examples/bs21"; TDIR="$REPO/examples/bs21/target/$TARGET/release"; PFX="bs21_"; BANNER="BS21" ;;
    bs20)            MANIFEST="$REPO/examples/bs20/Cargo.toml"; WS="examples/bs20"; TDIR="$REPO/examples/bs20/target/$TARGET/release"; PFX="bs20_"; BANNER="BS20" ;;
    *) echo "FATAL: unknown chip '$CHIP'"; exit 2 ;;
esac
SET="uart_hello timer_irq gpio_irq reset_demo spi_loopback i2c_scan"
[ -n "$EXAMPLE" ] && SET="$EXAMPLE"

# shared marker table (HIL truth) + which rows are timing-sensitive (QEMU can't prove)
marker_for() { case "$1" in
    uart_hello) echo "Hello from $BANNER" ;; timer_irq) echo "timer irq #|OK: timer" ;;
    gpio_irq) echo "gpio irq #" ;; reset_demo) echo "reset_reason=Software" ;;
    spi_loopback) echo "SPI loopback OK" ;; i2c_scan) echo "scan done|no devices" ;; *) echo "" ;; esac; }
timing_note() { case "$1" in
    uart_hello) echo "← 160 MHz baud base" ;; timer_irq) echo "← 24 MHz TCXO period" ;; *) echo "" ;; esac; }

# ── QEMU side: locate/boot the emulator, capture UART ─────────────────────────
WS63_QEMU="${WS63_QEMU:-/root/ws63-qemu}"
QEMU_BIN="${QEMU_BIN:-}"; [ -z "$QEMU_BIN" ] && { command -v qemu-system-riscv32 >/dev/null 2>&1 && QEMU_BIN=qemu-system-riscv32 || QEMU_BIN="$WS63_QEMU/qemu/build/qemu-system-riscv32"; }
qemu_ok=1; [ -x "$QEMU_BIN" ] || command -v "$QEMU_BIN" >/dev/null 2>&1 || qemu_ok=0
build() { if [ -z "$WS" ]; then ( cd "$REPO" && cargo build --release -p "$1" ); else ( cd "$REPO" && cargo build --manifest-path "$MANIFEST" --release ); fi; }
qemu_run() { local elf="$1"; timeout "$TIMEOUT" "$QEMU_BIN" -M "$CHIP" -nographic -bios none -kernel "$elf" </dev/null 2>&1; }

# ── HIL side: is the rig ready? (preflight, no writes) ────────────────────────
hil_ready=0
bash "$SKILLS/hil-smoke/hil.sh" "$CHIP" --preflight >/dev/null 2>&1 && hil_ready=1

echo "════════ qemu-vs-hil parity: chip=$CHIP  qemu=$([ $qemu_ok = 1 ] && echo ok || echo MISSING)  hil=$([ $hil_ready = 1 ] && echo ready || echo 'n/a (no board)') ════════"
printf "  %-14s %-8s %-8s %-7s %s\n" example QEMU HIL match note
printf "  %-14s %-8s %-8s %-7s %s\n" "-------" "----" "---" "-----" "----"

div=0
for ex in $SET; do
    pat="$(marker_for "$ex")"; note="$(timing_note "$ex")"
    build "$ex" >/dev/null 2>&1 || { printf "  %-14s %-8s %-8s %-7s %s\n" "$ex" "build!" "-" "-" "$note"; div=1; continue; }
    elf="$TDIR/${PFX}${ex}"; [ -f "$elf" ] || elf="$TDIR/${ex}"
    # QEMU
    q="n/a"
    if [ "$qemu_ok" = 1 ] && [ -f "$elf" ]; then
        out="$(qemu_run "$elf")"
        [ -n "$pat" ] && { echo "$out" | grep -qE "$pat" && q=PASS || q=FAIL; } || q="(none)"
    fi
    # HIL
    h="n/a"
    if [ "$hil_ready" = 1 ]; then
        if bash "$SKILLS/hil-smoke/hil.sh" "$CHIP" "$ex" >"/tmp/parity.$ex.out" 2>&1; then h=PASS; else h=FAIL; fi
    fi
    # verdict
    m="—"
    if [ "$q" = PASS ] && [ "$h" = PASS ]; then m="✓"
    elif [ "$h" = n/a ]; then m="qemu-only"
    elif [ "$q" != "$h" ]; then m="DIVERGE"; div=1; fi
    printf "  %-14s %-8s %-8s %-7s %s\n" "$ex" "$q" "$h" "$m" "$note"
done

echo ""
if [ "$hil_ready" = 0 ]; then
    echo "  HIL unavailable (no board / preflight NOT-READY) — QEMU column only."
    echo "  Run \`hil-smoke $CHIP --preflight\` to see what the rig needs."
    exit 0
fi
[ "$div" = 0 ] && { echo "  PARITY: QEMU ≡ silicon on all markers ✓"; exit 0; } \
               || { echo "  PARITY: DIVERGENCE found — feed the failing UART log to the hil-triage subagent."; exit 1; }
