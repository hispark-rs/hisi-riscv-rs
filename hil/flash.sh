#!/usr/bin/env bash
# Flash a ws63-rs firmware to a real WS63 board over serial, via the `hisiflash`
# CLI. The board's boot chain needs a vendor LoaderBoot pushed first, which
# `hisiflash write-program` does before writing the program.
#
# Usage:
#   hil/flash.sh <program.bin | program.elf | example-name> [port]
#   PORT=/dev/ttyUSB0 LOADERBOOT=/path/loaderboot.bin ADDRESS=0x200000 hil/flash.sh blinky
#
# Env:
#   PORT        serial port (exported as HISIFLASH_PORT). Auto-detected if unset.
#   BAUD        flash baud (HISIFLASH_BAUD; hisiflash default 921600).
#   LOADERBOOT  vendor LoaderBoot binary — REQUIRED. From fbb_ws63
#               (src/output/ws63/.../*loaderboot*.bin). Pushed before the program.
#   ADDRESS     flash offset for the program (REQUIRED; e.g. the app partition
#               offset — VERIFY against the board's partition table, see README.md).
#   HISIFLASH   the hisiflash binary (default `hisiflash` in PATH; install with
#               `cargo install hisiflash-cli` or build /root/hisiflash).
#   WS63_RS     ws63-rs checkout (default: parent of this script's dir).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WS63_RS="${WS63_RS:-$HERE}"
HISIFLASH="${HISIFLASH:-hisiflash}"
TARGET_DIR="$WS63_RS/target/riscv32imfc-unknown-none-elf/release"

ARG="${1:?usage: flash.sh <program.bin|program.elf|example-name> [port]}"
[ -n "${2:-}" ] && export HISIFLASH_PORT="$2"
[ -n "${PORT:-}" ] && export HISIFLASH_PORT="$PORT"
[ -n "${BAUD:-}" ] && export HISIFLASH_BAUD="$BAUD"

command -v "$HISIFLASH" >/dev/null 2>&1 || {
    echo "ERROR: '$HISIFLASH' not found — \`cargo install hisiflash-cli\` (or build /root/hisiflash)." >&2; exit 1; }
: "${LOADERBOOT:?set LOADERBOOT=<vendor loaderboot.bin> (from fbb_ws63 output)}"
[ -f "$LOADERBOOT" ] || { echo "ERROR: LOADERBOOT not found: $LOADERBOOT" >&2; exit 1; }
: "${ADDRESS:?set ADDRESS=<program flash offset, e.g. 0x200000> (verify vs partition table)}"

# Resolve the program to a .bin: accept a path, or an example name (prefer its
# .bin; objcopy the ELF if only that exists — rust-objcopy ships in the hisi-riscv toolchain).
resolve_bin() {
    local a="$1"
    case "$a" in
        *.bin) [ -f "$a" ] && { echo "$a"; return; } ;;
        *.elf) _objcopy "$a" "${a%.elf}.bin"; echo "${a%.elf}.bin"; return ;;
    esac
    [ -f "$TARGET_DIR/$a.bin" ] && { echo "$TARGET_DIR/$a.bin"; return; }
    if [ -f "$TARGET_DIR/$a" ]; then _objcopy "$TARGET_DIR/$a" "$TARGET_DIR/$a.bin"; echo "$TARGET_DIR/$a.bin"; return; fi
    echo "ERROR: cannot resolve firmware '$a' (looked in $TARGET_DIR)" >&2; exit 1
}
_objcopy() {
    local objcopy; objcopy="$(rustc +hisi-riscv --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-objcopy"
    command -v "$objcopy" >/dev/null 2>&1 || objcopy="rust-objcopy"
    echo "==> objcopy $1 -> $2" >&2
    "$objcopy" -O binary "$1" "$2"
}

BIN="$(resolve_bin "$ARG")"
echo "==> flashing $BIN  (loaderboot=$LOADERBOOT, address=$ADDRESS, port=${HISIFLASH_PORT:-auto})"
exec "$HISIFLASH" write-program --loaderboot "$LOADERBOOT" "$BIN" --address "$ADDRESS"
