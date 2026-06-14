#!/usr/bin/env bash
# Flash a ws63-rs firmware to a real WS63 board. *** Hardware-validated 2026-06-14
# on real WS63 silicon (blinky boots + blinks GPIO0). ***
#
# Two paths — pick one with METHOD=:
#
#   METHOD=probe-rs  (DEFAULT, the VALIDATED path)
#       Builds the 0x300-header app image via hil/pack.sh, then writes it to the
#       app partition in XIP flash with `probe-rs download` and resets. Needs the
#       PATCHED FORK github.com/hispark-rs/probe-rs branch add-hisilicon-ws63-bs21
#       (the WS63 target + ws63-sfc flash algo are NOT in upstream probe-rs yet)
#       plus its HiSilicon_WS63.yaml chip description (PROBE_RS_YAML=).
#
#   METHOD=hisiflash  (vendor serial/YMODEM path)
#       Pushes a vendor LoaderBoot, then writes the program with `hisiflash
#       write-program`. (Or pack a .fwpkg with `FWPKG=1 hil/pack.sh` and
#       `hisiflash flash <out.fwpkg>` — see hil/README.md.)
#
# Usage:
#   hil/flash.sh <program.elf | program.bin | example-name> [port]
#   PROBE_RS_YAML=/path/HiSilicon_WS63.yaml hil/flash.sh blinky          # probe-rs (default)
#   METHOD=hisiflash PORT=/dev/ttyUSB0 LOADERBOOT=/path/loaderboot.bin ADDRESS=0x230000 \
#       hil/flash.sh blinky                                              # vendor path
#
# Env (probe-rs path):
#   CHIP          probe-rs --chip target (default WS63).
#   PROBE_RS_YAML chip-description YAML from the fork (REQUIRED — HiSilicon_WS63.yaml).
#   BASE_ADDRESS  app-partition flash address (default 0x00230000 ws63 / 0x00090000 bs21).
#   PROBE_RS      the probe-rs binary (default `probe-rs` in PATH).
# Env (hisiflash path):
#   PORT          serial port (exported as HISIFLASH_PORT). Auto-detected if unset.
#   BAUD          flash baud (HISIFLASH_BAUD; hisiflash default 921600).
#   LOADERBOOT    vendor LoaderBoot binary — REQUIRED. From fbb_ws63
#                 (src/output/ws63/.../*loaderboot*.bin). Pushed before the program.
#   ADDRESS       flash offset for the program (REQUIRED; app partition offset —
#                 VERIFY against the board's partition table, see README.md).
#   HISIFLASH     the hisiflash binary (default `hisiflash` in PATH).
# Shared:
#   CHIP_KIND     ws63|bs21 (default ws63) — selects the default app-partition addr.
#   WS63_RS       ws63-rs checkout (default: parent of this script's dir).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WS63_RS="${WS63_RS:-$HERE}"
METHOD="${METHOD:-probe-rs}"
CHIP_KIND="${CHIP_KIND:-ws63}"
TARGET_DIR="$WS63_RS/examples/ws63/target/riscv32imfc-unknown-none-elf/release"

ARG="${1:?usage: flash.sh <program.elf|program.bin|example-name> [port]}"

case "$CHIP_KIND" in bs21) DEF_ADDR=0x00090000 ;; *) DEF_ADDR=0x00230000 ;; esac

if [ "$METHOD" = "probe-rs" ]; then
    # VALIDATED path: build the 0x300-header image, then probe-rs download + reset.
    PROBE_RS="${PROBE_RS:-probe-rs}"
    CHIP="${CHIP:-WS63}"
    BASE_ADDRESS="${BASE_ADDRESS:-$DEF_ADDR}"
    command -v "$PROBE_RS" >/dev/null 2>&1 || {
        echo "ERROR: '$PROBE_RS' not found — install the PATCHED fork" >&2
        echo "       github.com/hispark-rs/probe-rs (branch add-hisilicon-ws63-bs21);" >&2
        echo "       upstream probe-rs has no WS63 target / ws63-sfc flash algo yet." >&2
        exit 1; }
    : "${PROBE_RS_YAML:?set PROBE_RS_YAML=<HiSilicon_WS63.yaml from the hispark-rs/probe-rs fork>}"
    [ -f "$PROBE_RS_YAML" ] || { echo "ERROR: PROBE_RS_YAML not found: $PROBE_RS_YAML" >&2; exit 1; }

    # Produce the bootable .img (0x300 header || body) via pack.sh.
    IMG="$TARGET_DIR/$(basename "${ARG%.*}").img"
    CHIP="$CHIP_KIND" "$HERE/hil/pack.sh" "$ARG" "$IMG" >&2

    echo "==> probe-rs download $IMG -> chip=$CHIP @ $BASE_ADDRESS (yaml=$PROBE_RS_YAML)"
    "$PROBE_RS" download --chip "$CHIP" --chip-description-path "$PROBE_RS_YAML" \
        --binary-format bin --base-address "$BASE_ADDRESS" "$IMG"
    echo "==> probe-rs reset"
    exec "$PROBE_RS" reset --chip "$CHIP" --chip-description-path "$PROBE_RS_YAML"
fi

# ---- vendor hisiflash path ----
HISIFLASH="${HISIFLASH:-hisiflash}"
[ -n "${2:-}" ] && export HISIFLASH_PORT="$2"
[ -n "${PORT:-}" ] && export HISIFLASH_PORT="$PORT"
[ -n "${BAUD:-}" ] && export HISIFLASH_BAUD="$BAUD"

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

command -v "$HISIFLASH" >/dev/null 2>&1 || {
    echo "ERROR: '$HISIFLASH' not found — \`cargo install hisiflash-cli\` (or build /root/hisiflash)." >&2; exit 1; }
: "${LOADERBOOT:?set LOADERBOOT=<vendor loaderboot.bin> (from fbb_ws63 output)}"
[ -f "$LOADERBOOT" ] || { echo "ERROR: LOADERBOOT not found: $LOADERBOOT" >&2; exit 1; }
: "${ADDRESS:?set ADDRESS=<program flash offset, e.g. 0x230000> (verify vs partition table)}"

BIN="$(resolve_bin "$ARG")"
echo "==> flashing $BIN  (loaderboot=$LOADERBOOT, address=$ADDRESS, port=${HISIFLASH_PORT:-auto})"
exec "$HISIFLASH" write-program --loaderboot "$LOADERBOOT" "$BIN" --address "$ADDRESS"
