#!/usr/bin/env bash
# Pack a ws63-rs program (ELF or .bin) into the canonical app flash image
# (+ optional fwpkg). Image/header/hash/body range semantics come from the
# `hisi-fwpkg plan` library path; this script is only a runner wrapper.
#
# *** Hardware-validated 2026-06-14 on real WS63 silicon (blinky boots + blinks GPIO0). ***
#
# By default this emits the raw .img plus a .plan.json sidecar — that is the
# artifact the probe-rs-download path flashes (see below). Pass FWPKG=1 to also
# build the single-partition .fwpkg for the vendor hisiflash/YMODEM path.
#
# Usage:
#   hil/pack.sh <program.elf | program.bin | example-name> [output.img]
#   CHIP=ws63 hil/pack.sh blinky               # -> target/.../blinky.img
#   FWPKG=1 hil/pack.sh blinky                 # also emit target/.../blinky.fwpkg
#
# Env:
#   CHIP        target chip (ws63|bs21). Default ws63. Sets app partition addr.
#   APP_ADDR    override the app partition flash address (e.g. 0x230000).
#   FWPKG       set non-empty to also emit a .fwpkg (vendor hisiflash path).
#   HISI_FWPKG  the hisi-fwpkg binary (default `hisi-fwpkg` in PATH; install with
#               `cargo install hisi-fwpkg-cli` or build the hisi-fwpkg repo).
#   WS63_RS     ws63-rs checkout (default: parent of this script's dir).
#
# Two ways to flash the produced .img — see the "FLASH" notes printed at the end
# and hil/README.md. Short version:
#   A) probe-rs (VALIDATED, needs the hispark-rs/probe-rs FORK):
#        probe-rs download --chip WS63 --chip-description-path HiSilicon_WS63.yaml \
#            --binary-format bin --base-address 0x00230000 <out.img>
#        probe-rs reset --chip WS63 --chip-description-path HiSilicon_WS63.yaml
#   B) vendor: FWPKG=1 hil/pack.sh ... ; then `hisiflash flash <out.fwpkg>`.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WS63_RS="${WS63_RS:-$HERE}"
HISI_FWPKG="${HISI_FWPKG:-hisi-fwpkg}"
CHIP="${CHIP:-ws63}"
TARGET_DIR="$WS63_RS/examples/ws63/target/riscv32imfc-unknown-none-elf/release"
ROOT_TARGET_DIR="$WS63_RS/target/riscv32imfc-unknown-none-elf/release"

ARG="${1:?usage: pack.sh <program.elf|program.bin|example-name> [output.img]}"

command -v "$HISI_FWPKG" >/dev/null 2>&1 || {
    echo "ERROR: '$HISI_FWPKG' not found — \`cargo install hisi-fwpkg-cli\` (https://github.com/hispark-rs/hisi-fwpkg)." >&2
    exit 1
}

# Resolve the program: accept an ELF/bin path, or an example name (its ELF in
# the release target dir). hisi-fwpkg auto-detects ELF vs raw bin from the magic.
resolve_input() {
    local a="$1"
    case "$a" in
        *.elf | *.bin) [ -f "$a" ] && { echo "$a"; return; } ;;
    esac
    [ -f "$ROOT_TARGET_DIR/$a" ] && { echo "$ROOT_TARGET_DIR/$a"; return; }
    [ -f "$ROOT_TARGET_DIR/$a.bin" ] && { echo "$ROOT_TARGET_DIR/$a.bin"; return; }
    [ -f "$TARGET_DIR/$a" ] && { echo "$TARGET_DIR/$a"; return; }
    [ -f "$TARGET_DIR/$a.bin" ] && { echo "$TARGET_DIR/$a.bin"; return; }
    echo "ERROR: cannot resolve program '$a' (looked in $ROOT_TARGET_DIR and $TARGET_DIR)" >&2
    exit 1
}

INPUT="$(resolve_input "$ARG")"
BASE="$(basename "${INPUT%.*}")"
IMG="${2:-$(dirname "$INPUT")/$BASE.img}"

ADDR_ARGS=()
[ -n "${APP_ADDR:-}" ] && ADDR_ARGS=(--app-addr "$APP_ADDR")

# Primary artifact: the complete flash image and its canonical plan. This is
# what the probe-rs-download path flashes to the app partition as a plain bin.
PLAN="${IMG%.img}.plan.json"
echo "==> plan $INPUT -> $IMG (+ $PLAN)"
"$HISI_FWPKG" plan "$INPUT" --chip "$CHIP" "${ADDR_ARGS[@]}" --image-output "$IMG" > "$PLAN"

if [ -n "${FWPKG:-}" ]; then
    OUT="${IMG%.img}.fwpkg"
    echo "==> pack canonical image $IMG -> $OUT  (chip=$CHIP${APP_ADDR:+, app_addr=$APP_ADDR})"
    # Keep one image-semantics path: `plan` expands ELF PT_LOAD segments and
    # patches the verified body; `pack` only wraps those exact bytes as a
    # partition. Re-planning the ELF here can drift from the emitted plan.
    "$HISI_FWPKG" pack "$IMG" -o "$OUT" --chip "$CHIP" "${ADDR_ARGS[@]}" --name "$BASE"
fi

BASE_ADDR="$(python3 - "$PLAN" <<'PY'
import json, sys
with open(sys.argv[1], "r", encoding="utf-8") as f:
    print(f"0x{json.load(f)['base_addr']:08X}")
PY
)"

echo "==> done: $IMG"
echo "    plan: $PLAN"
echo
echo "    FLASH (A) probe-rs — VALIDATED 2026-06-14, needs the hispark-rs/probe-rs FORK"
echo "             branch add-hisilicon-ws63-bs21-hil-baseline + its HiSilicon_WS63.yaml (WS63 target +"
echo "             ws63-sfc flash algo are NOT in upstream probe-rs yet):"
echo "      probe-rs download --chip WS63 --chip-description-path HiSilicon_WS63.yaml \\"
echo "          --binary-format bin --base-address $BASE_ADDR $IMG"
echo "      probe-rs reset --chip WS63 --chip-description-path HiSilicon_WS63.yaml"
echo
if [ -n "${FWPKG:-}" ]; then
echo "    FLASH (B) vendor hisiflash (YMODEM @230400):"
echo "      hisiflash info  ${IMG%.img}.fwpkg"
echo "      hisiflash flash ${IMG%.img}.fwpkg"
else
echo "    FLASH (B) vendor hisiflash: re-run with FWPKG=1 to also emit a .fwpkg, then"
echo "      hisiflash flash ${IMG%.img}.fwpkg"
fi
