#!/usr/bin/env bash
# Pack a ws63-rs program (ELF or .bin) into a WS63 app **image** + **fwpkg** so
# flashboot can actually load it. A bare ELF/bin at the app partition does NOT
# boot — flashboot expects a 0x300-byte HiSilicon image header in front of the
# code and jumps unconditionally to app_partition + 0x300. This script adds that
# header and wraps it in a fwpkg, using the `hisi-fwpkg` tool.
#
# Usage:
#   hil/pack.sh <program.elf | program.bin | example-name> [output.fwpkg]
#   CHIP=ws63 hil/pack.sh blinky               # -> target/.../blinky.fwpkg
#   IMAGE_ONLY=1 hil/pack.sh blinky            # also emit the raw .img (no fwpkg)
#
# Env:
#   CHIP        target chip (ws63|bs21). Default ws63. Sets app partition addr.
#   APP_ADDR    override the app partition flash address (e.g. 0x230000).
#   HISI_FWPKG  the hisi-fwpkg binary (default `hisi-fwpkg` in PATH; install with
#               `cargo install hisi-fwpkg-cli` or build the hisi-fwpkg repo).
#   WS63_RS     ws63-rs checkout (default: parent of this script's dir).
#
# Then flash the produced .fwpkg with: hisiflash flash <out.fwpkg>
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WS63_RS="${WS63_RS:-$HERE}"
HISI_FWPKG="${HISI_FWPKG:-hisi-fwpkg}"
CHIP="${CHIP:-ws63}"
TARGET_DIR="$WS63_RS/examples/ws63/target/riscv32imfc-unknown-none-elf/release"

ARG="${1:?usage: pack.sh <program.elf|program.bin|example-name> [output.fwpkg]}"

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
    [ -f "$TARGET_DIR/$a" ] && { echo "$TARGET_DIR/$a"; return; }
    [ -f "$TARGET_DIR/$a.bin" ] && { echo "$TARGET_DIR/$a.bin"; return; }
    echo "ERROR: cannot resolve program '$a' (looked in $TARGET_DIR)" >&2
    exit 1
}

INPUT="$(resolve_input "$ARG")"
BASE="$(basename "${INPUT%.*}")"
OUT="${2:-$TARGET_DIR/$BASE.fwpkg}"

ADDR_ARGS=()
[ -n "${APP_ADDR:-}" ] && ADDR_ARGS=(--app-addr "$APP_ADDR")

if [ -n "${IMAGE_ONLY:-}" ]; then
    IMG="${OUT%.fwpkg}.img"
    echo "==> image $INPUT -> $IMG"
    "$HISI_FWPKG" image "$INPUT" -o "$IMG"
fi

echo "==> pack $INPUT -> $OUT  (chip=$CHIP${APP_ADDR:+, app_addr=$APP_ADDR})"
"$HISI_FWPKG" pack "$INPUT" -o "$OUT" --chip "$CHIP" "${ADDR_ARGS[@]}" --name "$BASE"
echo "==> done: $OUT"
echo "    verify:  hisiflash info $OUT"
echo "    flash:   hisiflash flash $OUT"
