#!/usr/bin/env bash
# Cargo *runner* that flashes a freshly-built ELF to a real WS63 board and boots
# it — turning `cargo run` into "flash to hardware".
#
# Flow: hisi-fwpkg plan image → probe-rs bin download → JLink hardware nRST → UART capture.
# probe-rs `reset` (SC_SYS_RES) does not clean the SFC; only hardware nRST
# (POR) brings the SFC back to a state the boot ROM can initialise.
#
# Env (all optional, sensible defaults):
#   PROBE_RS     probe-rs binary                    (default: `probe-rs` in PATH)
#   PROBE_CHIP   probe-rs --chip value              (default WS63)
#   PROBE_YAML   --chip-description-path YAML       (default: empty = built-in DB)
#   PROBE_SPEED  debug transport speed in kHz       (default 2000)
#   HISI_FWPKG   hisi-fwpkg binary                  (default: `hisi-fwpkg` in PATH)
#   PORT         board UART0 to capture             (default: none = don't capture)
#   UART_BAUD    UART baud                          (default 115200)
#   MONITOR      seconds to capture UART            (default 10)
set -euo pipefail

ELF="${1:?cargo passes the built ELF path as \$1}"

PROBE_RS="${PROBE_RS:-probe-rs}"
PROBE_CHIP="${PROBE_CHIP:-WS63}"
PROBE_SPEED="${PROBE_SPEED:-2000}"
HISI_FWPKG="${HISI_FWPKG:-hisi-fwpkg}"
UART_BAUD="${UART_BAUD:-115200}"
MONITOR="${MONITOR:-10}"

# Resolve the real directory of this script (may be invoked via symlink).
SCRIPT_DIR="$(cd "$(dirname "$(realpath "${BASH_SOURCE[0]}")")" && pwd)"

command -v "$HISI_FWPKG" >/dev/null 2>&1 || {
    echo "run-hw: '$HISI_FWPKG' not found — install hisi-fwpkg (https://github.com/hispark-rs/hisi-fwpkg)." >&2
    exit 1
}
command -v "$PROBE_RS" >/dev/null 2>&1 || {
    echo "run-hw: '$PROBE_RS' not found — needs the patched fork (hispark-rs/probe-rs, branch add-hisilicon-ws63-bs21-hil-baseline)." >&2
    exit 1
}

yaml_args=()
[ -n "${PROBE_YAML:-}" ] && yaml_args=(--chip-description-path "$PROBE_YAML")

IMAGE="${ELF}.hisi.img"
PLAN="${ELF}.hisi-plan.json"
echo "run-hw: planning complete flash image: $(basename "$ELF") -> $(basename "$IMAGE")"
"$HISI_FWPKG" plan "$ELF" --chip ws63 --image-output "$IMAGE" > "$PLAN"
BASE_ADDRESS="$(python3 - "$PLAN" <<'PY'
import json, sys
with open(sys.argv[1], "r", encoding="utf-8") as f:
    print(f"0x{json.load(f)['base_addr']:08X}")
PY
)"

echo "run-hw: downloading planned image via probe-rs bin path @ $BASE_ADDRESS (${PROBE_SPEED} kHz)"
"$PROBE_RS" download --chip "$PROBE_CHIP" --speed "$PROBE_SPEED" "${yaml_args[@]}" \
    --verify --binary-format bin --base-address "$BASE_ADDRESS" "$IMAGE"

if [ -n "${PORT:-}" ]; then
    echo "run-hw: nRST + capturing $PORT @ ${UART_BAUD} for ${MONITOR}s"
    PORT="$PORT" UART_BAUD="$UART_BAUD" MONITOR="$MONITOR" \
        uv run "$SCRIPT_DIR/nrst-and-capture.py"
fi

echo "run-hw: done."
