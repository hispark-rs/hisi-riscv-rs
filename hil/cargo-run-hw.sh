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
#   PROBE_RS_PROBE probe-rs --probe selector         (required when multiple probes exist)
#   PROBE_CHIP   probe-rs --chip value              (default WS63)
#   PROBE_YAML   --chip-description-path YAML       (default: empty = built-in DB)
#   PROBE_SPEED  debug transport speed in kHz       (default 2000)
#   PROBE_DOWNLOAD_ATTEMPTS                          (default 2)
#   PROBE_RETRY_SPEED retry transport speed in kHz   (default 500)
#   HISI_FWPKG   hisi-fwpkg binary                  (default: `hisi-fwpkg` in PATH)
#   PORT         board UART0 to capture             (default: none = don't capture)
#   UART_BAUD    UART baud                          (default 115200)
#   MONITOR      seconds to capture UART            (default 10)
#   JLINK_SERIAL J-Link serial used for hardware nRST (required when multiple J-Links exist)
set -euo pipefail

ELF="${1:?cargo passes the built ELF path as \$1}"

PROBE_RS="${PROBE_RS:-probe-rs}"
PROBE_CHIP="${PROBE_CHIP:-WS63}"
PROBE_SPEED="${PROBE_SPEED:-2000}"
PROBE_DOWNLOAD_ATTEMPTS="${PROBE_DOWNLOAD_ATTEMPTS:-2}"
PROBE_RETRY_SPEED="${PROBE_RETRY_SPEED:-500}"
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

IMAGE="${ELF}.hisi.img"
PLAN="${ELF}.hisi-plan.json"
echo "run-hw: planning complete flash image: $(basename "$ELF") -> $(basename "$IMAGE")"
"$HISI_FWPKG" plan "$ELF" --chip ws63 --image-output "$IMAGE" > "$PLAN"
BASE_ADDRESS="$(uv run "$SCRIPT_DIR/../scripts/read-flash-plan-base.py" "$PLAN")"

case "$PROBE_DOWNLOAD_ATTEMPTS" in
    ''|*[!0-9]*|0)
        echo "run-hw: PROBE_DOWNLOAD_ATTEMPTS must be a positive integer" >&2
        exit 2
        ;;
esac

download_ok=0
for ((attempt = 1; attempt <= PROBE_DOWNLOAD_ATTEMPTS; attempt++)); do
    speed="$PROBE_SPEED"
    [ "$attempt" -eq 1 ] || speed="$PROBE_RETRY_SPEED"
    echo "run-hw: downloading planned image via probe-rs bin path @ $BASE_ADDRESS (${speed} kHz, attempt ${attempt}/${PROBE_DOWNLOAD_ATTEMPTS})"
    probe_args=(download --non-interactive --chip "$PROBE_CHIP" --speed "$speed")
    [ -z "${PROBE_RS_PROBE:-}" ] || probe_args+=(--probe "$PROBE_RS_PROBE")
    [ -z "${PROBE_YAML:-}" ] || probe_args+=(--chip-description-path "$PROBE_YAML")
    probe_args+=(--verify --binary-format bin --base-address "$BASE_ADDRESS" "$IMAGE")
    if "$PROBE_RS" "${probe_args[@]}"; then
        download_ok=1
        break
    fi
    if [ "$attempt" -lt "$PROBE_DOWNLOAD_ATTEMPTS" ]; then
        echo "run-hw: download failed; restoring target with hardware nRST before retry" >&2
        uv run "$SCRIPT_DIR/nrst-and-capture.py" --reset-only
        sleep 1
    fi
done

if [ "$download_ok" -ne 1 ]; then
    echo "run-hw: download failed after ${PROBE_DOWNLOAD_ATTEMPTS} verified attempts" >&2
    exit 1
fi

if [ -n "${PORT:-}" ]; then
    echo "run-hw: nRST + capturing $PORT @ ${UART_BAUD} for ${MONITOR}s"
    PORT="$PORT" UART_BAUD="$UART_BAUD" MONITOR="$MONITOR" \
        uv run "$SCRIPT_DIR/nrst-and-capture.py"
fi

echo "run-hw: done."
