#!/usr/bin/env bash
# qemu-smoke driver — build ws63-rs example firmware for a chip and boot it in the
# hisi-riscv-qemu fork, asserting the expected UART/GPIO behaviour.
#
#   bash smoke.sh <chip> [example]
#     chip:     ws63 | bs21 | bs21e | bs22 | bs20
#     example:  blinky | uart_hello | timer_irq | gpio_irq | embassy_multitask | ...
#               (omit to build + run the chip's FULL smoke suite — the validated set)
#
# Two modes:
#   - no example  → build the chip's example set, then delegate to ws63-qemu's
#                   per-chip smoke script (the source of truth for assertions).
#   - one example → build just that example, boot it under `-M <chip>`, and apply a
#                   focused assertion (banner / GPIO toggle / embassy interleave / IRQ).
#
# Env overrides:
#   WS63_QEMU  sibling QEMU fork checkout (default: autodetect /root/ws63-qemu | ../ws63-qemu)
#   QEMU_BIN   qemu-system-riscv32 binary (default: PATH, else $WS63_QEMU/qemu/build/…; built if absent)
#   TIMEOUT    seconds to run each boot before killing (default 5)
#   PROFILE    release | debug (default release)
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"            # .claude/skills/qemu-smoke → repo root
TARGET=riscv32imfc-unknown-none-elf
PROFILE="${PROFILE:-release}"
TIMEOUT="${TIMEOUT:-5}"

CHIP="${1:-}"
EXAMPLE="${2:-}"
[ -n "$CHIP" ] || { echo "usage: smoke.sh <chip> [example]   chip ∈ ws63|bs21|bs21e|bs22|bs20"; exit 2; }

# ── locate the QEMU fork + binary (build it once if missing) ─────────────────
WS63_QEMU="${WS63_QEMU:-}"
if [ -z "$WS63_QEMU" ]; then
    for c in /root/ws63-qemu "$REPO/../ws63-qemu" "$REPO/../hisi-riscv-qemu"; do
        [ -d "$c" ] && { WS63_QEMU="$c"; break; }
    done
fi
QEMU_BIN="${QEMU_BIN:-}"
if [ -z "$QEMU_BIN" ]; then
    if command -v qemu-system-riscv32 >/dev/null 2>&1; then
        QEMU_BIN="$(command -v qemu-system-riscv32)"
    else
        QEMU_BIN="$WS63_QEMU/qemu/build/qemu-system-riscv32"
    fi
fi
if [ ! -x "$QEMU_BIN" ]; then
    echo "==> QEMU not found at $QEMU_BIN — building via $WS63_QEMU/scripts/build.sh"
    [ -d "$WS63_QEMU" ] || { echo "FATAL: no QEMU fork (set WS63_QEMU=)"; exit 2; }
    ( cd "$WS63_QEMU" && ./scripts/build.sh ) || { echo "FATAL: qemu build failed"; exit 2; }
    QEMU_BIN="$WS63_QEMU/qemu/build/qemu-system-riscv32"
fi
"$QEMU_BIN" -M help 2>/dev/null | grep -qiE "^$CHIP " || {
    echo "FATAL: QEMU ($QEMU_BIN) has no '-M $CHIP' machine. Built machines:"
    "$QEMU_BIN" -M help 2>/dev/null | grep -iE "ws63|bs2|bs21" | sed 's/^/    /'
    exit 2
}

# ── chip → build manifest + target dir + bin prefix + ws63-qemu smoke script ──
# bs21e/bs22 reuse the examples/bs21 (chip-bs21) binaries, booted under their own
# machine; bs20 has its own dir (128K memory.x). The bin name carries the chip
# prefix only for the isolated bs2x workspaces (e.g. bs21_uart_hello); ws63
# examples keep the bare crate name (uart_hello).
case "$CHIP" in
    ws63)
        MANIFEST="$REPO/Cargo.toml"; WS=""
        TDIR="$REPO/target/$TARGET/$PROFILE"; PFX=""
        SET="blinky uart_hello timer_irq gpio_irq reset_demo spi_loopback i2c_scan embassy_multitask" ;;
    bs21|bs21e|bs22)
        MANIFEST="$REPO/examples/bs21/Cargo.toml"; WS="examples/bs21"
        TDIR="$REPO/examples/bs21/target/$TARGET/$PROFILE"; PFX="bs21_"
        SET="blinky uart_hello spi_loopback i2c_scan" ;;
    bs20)
        MANIFEST="$REPO/examples/bs20/Cargo.toml"; WS="examples/bs20"
        TDIR="$REPO/examples/bs20/target/$TARGET/$PROFILE"; PFX="bs20_"
        SET="blinky uart_hello spi_loopback i2c_scan" ;;
    *) echo "FATAL: unknown chip '$CHIP' (ws63|bs21|bs21e|bs22|bs20)"; exit 2 ;;
esac
PROFLAG=""; [ "$PROFILE" = release ] && PROFLAG="--release"

build() {  # build the given crate(s); ws63 via -p in the root ws, bs2x via manifest
    local crates="$1"
    if [ -z "$WS" ]; then
        local pflags=""; for c in $crates; do pflags="$pflags -p $c"; done
        echo "==> cargo build $PROFLAG$pflags  (ws63 workspace)"
        ( cd "$REPO" && cargo build $PROFLAG $pflags ) || return 1
    else
        # isolated bs2x workspace — build the whole small workspace in one shot
        echo "==> cargo build --manifest-path $WS/Cargo.toml $PROFLAG  (bs2x isolated workspace)"
        ( cd "$REPO" && cargo build --manifest-path "$MANIFEST" $PROFLAG ) || return 1
    fi
}

run() {  # boot one ELF under -M $CHIP, capture output (UART on stdio) + optional gpio trace
    local elf="$1" trace="${2:-}" out
    local args=(-M "$CHIP" -nographic -bios none -kernel "$elf")
    [ -n "$trace" ] && args=(-M "$CHIP" -nographic -bios none --trace "$trace" -kernel "$elf")
    timeout "$TIMEOUT" "$QEMU_BIN" "${args[@]}" </dev/null 2>&1
}

assert() {  # focused per-example assertion; echoes PASS/FAIL, returns 0/1
    local ex="$1" elf="$2" out rc=1
    case "$ex" in
        uart_hello)
            out="$(run "$elf")"
            if echo "$out" | grep -qiE "hello.*$CHIP|UART0 .* alive|on QEMU"; then
                echo "  PASS uart_hello — banner: $(echo "$out" | grep -m1 -iE 'hello|alive')"; rc=0
            else echo "  FAIL uart_hello — no banner. First lines:"; echo "$out" | head -3 | sed 's/^/    /'; fi ;;
        blinky)
            out="$(run "$elf" 'ws63_gpio_*')"
            local n; n=$(echo "$out" | grep -c "ws63_gpio_")
            if [ "$n" -gt 1 ] && ! echo "$out" | grep -qiE "illegal|fault|abort"; then
                echo "  PASS blinky — $n GPIO toggle events, no fault"; rc=0
            else echo "  FAIL blinky — gpio_events=$n. First lines:"; echo "$out" | head -3 | sed 's/^/    /'; fi ;;
        async_*|embassy_*)
            out="$(run "$elf")"
            if echo "$out" | grep -q "\[fast\]" && echo "$out" | grep -q "\[slow\]"; then
                echo "  PASS $ex — embassy [fast]/[slow] interleave"; rc=0
            else echo "  FAIL $ex — no interleave. First lines:"; echo "$out" | head -3 | sed 's/^/    /'; fi ;;
        timer_irq)
            out="$(run "$elf")"
            if echo "$out" | grep -qi "timer interrupts delivered"; then echo "  PASS timer_irq"; rc=0
            else echo "  FAIL timer_irq. First lines:"; echo "$out" | head -3 | sed 's/^/    /'; fi ;;
        gpio_irq)
            out="$(run "$elf")"
            if echo "$out" | grep -qi "local IRQ (>=32) delivered"; then echo "  PASS gpio_irq"; rc=0
            else echo "  FAIL gpio_irq. First lines:"; echo "$out" | head -3 | sed 's/^/    /'; fi ;;
        *)
            out="$(run "$elf")"
            echo "  INFO $ex — no specific assertion; first UART lines:"; echo "$out" | head -6 | sed 's/^/    /'
            echo "$out" | grep -qiE "illegal|fault|abort|panic" && { echo "  FAIL $ex — fault/panic seen"; rc=1; } || rc=0 ;;
    esac
    return $rc
}

echo "════════ qemu-smoke: chip=$CHIP  qemu=$QEMU_BIN  profile=$PROFILE ════════"

# ── single-example mode ──────────────────────────────────────────────────────
if [ -n "$EXAMPLE" ]; then
    build "$EXAMPLE" || { echo "FATAL: build failed for $EXAMPLE"; exit 1; }
    ELF="$TDIR/${PFX}${EXAMPLE}"
    [ -f "$ELF" ] || ELF="$TDIR/${EXAMPLE}"     # ws63 has no prefix; bs2x prefixed
    [ -f "$ELF" ] || { echo "FATAL: built ELF not found ($TDIR/${PFX}${EXAMPLE})"; exit 1; }
    assert "$EXAMPLE" "$ELF"; exit $?
fi

# ── full-suite mode: build the set, then delegate to ws63-qemu's smoke script ──
build "$SET" || { echo "FATAL: build failed for set: $SET"; exit 1; }
SCRIPT="$WS63_QEMU/scripts/smoke-test.sh"
[ "$CHIP" != ws63 ] && SCRIPT="$WS63_QEMU/scripts/${CHIP}-smoke-test.sh"
if [ -x "$SCRIPT" ]; then
    echo "==> delegating assertions to $(basename "$SCRIPT") (source of truth)"
    QEMU_BIN="$QEMU_BIN" WS63_RS="$REPO" bash "$SCRIPT"; exit $?
fi
# fallback: no per-chip script — assert each built example inline
echo "==> no $(basename "$SCRIPT"); running inline assertions"
fail=0
for ex in $SET; do
    ELF="$TDIR/${PFX}${ex}"; [ -f "$ELF" ] || ELF="$TDIR/${ex}"
    [ -f "$ELF" ] && { assert "$ex" "$ELF" || fail=1; } || echo "  SKIP $ex (not built)"
done
exit $fail
