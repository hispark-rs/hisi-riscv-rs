#!/usr/bin/env bash
# Build the final public-facade connectivity image, flash it once, then prove
# both its response bound and end-to-end network contract across unchanged-image
# J-Link nRST boots.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/ws63-wifi-credentials.sh
source "$HERE/hil/lib/ws63-wifi-credentials.sh"

PORT="${PORT:-}"
PROBE_SPEED="${PROBE_SPEED:-3000}"
RUNS="${RUNS:-20}"
TIMEOUT="${TIMEOUT:-90}"
MAX_RUNNER_STEP_MS="${MAX_RUNNER_STEP_MS:-100}"
PROFILE_ID="${A5B_PROFILE_ID:-wifi-connectivity-upstream-wpa2}"
FEATURES="${A5B_FEATURES:-wpa2,data-path-diagnostics}"

usage() {
    cat <<'EOF'
Usage:
  WS63_WIFI_ENV_FILE=/path/to/local-0600.env \
  PORT=/dev/cu.wchusbserial... hil/ws63-a5b-response-bound.sh

The credential file uses the same local-secret contract as connectivity smoke:
exactly WS63_WIFI_SSID=... and WS63_WIFI_PASSPHRASE=..., owned by the current
user, mode 0600, no symlink. It is retained by default; set
WS63_WIFI_ENV_FILE_DISPOSITION=delete for a one-shot file.

Optional:
  PROBE_RS, PROBE_YAML, PROBE_CHIP, PROBE_SPEED (default 3000),
  PROBE_DOWNLOAD_ATTEMPTS, PROBE_RETRY_SPEED, HISI_FWPKG,
  RUNS (default 20), TIMEOUT (default 90 seconds),
  MAX_RUNNER_STEP_MS (default 100),
  A5B_FEATURES (default wpa2,data-path-diagnostics),
  A5B_PROFILE_ID (default wifi-connectivity-upstream-wpa2),
  EVIDENCE_DIR (default timestamped directory under /private/tmp).
EOF
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "ERROR: required command not found: $1" >&2
        return 1
    }
}

positive_integer() {
    case "$1" in
        ''|*[!0-9]*|0) return 1 ;;
        *) return 0 ;;
    esac
}

preflight() {
    local failed=0
    for command in cargo JLinkExe "${PROBE_RS:-probe-rs}" \
        "${HISI_FWPKG:-hisi-fwpkg}" uv; do
        require_command "$command" || failed=1
    done
    if [ -z "$PORT" ]; then
        echo "ERROR: set PORT to the WS63 UART0 device" >&2
        failed=1
    elif [ ! -e "$PORT" ]; then
        echo "ERROR: UART device does not exist: $PORT" >&2
        failed=1
    fi
    if [ -z "${WS63_WIFI_SSID:-}" ]; then
        echo "ERROR: set WS63_WIFI_SSID through the HIL secret store" >&2
        failed=1
    fi
    if [ -z "${WS63_WIFI_PASSPHRASE:-}" ]; then
        echo "ERROR: set WS63_WIFI_PASSPHRASE through the HIL secret store" >&2
        failed=1
    fi
    if ! positive_integer "$RUNS"; then
        echo "ERROR: RUNS must be a positive integer" >&2
        failed=1
    fi
    if ! positive_integer "$TIMEOUT"; then
        echo "ERROR: TIMEOUT must be a positive integer" >&2
        failed=1
    fi
    if ! positive_integer "$MAX_RUNNER_STEP_MS"; then
        echo "ERROR: MAX_RUNNER_STEP_MS must be a positive integer" >&2
        failed=1
    fi
    [ "$failed" -eq 0 ] || return 1
    echo "a5b-response-bound: preflight PASS (runs=$RUNS, probe=${PROBE_SPEED}kHz, budget=${MAX_RUNNER_STEP_MS}ms)"
}

case "${1:-}" in
    -h|--help) usage; exit 0 ;;
esac

load_ws63_wifi_credentials

case "${1:-}" in
    --preflight) preflight; exit $? ;;
    "") ;;
    *) usage >&2; exit 2 ;;
esac

preflight

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
TARGET_DIR="$TMP/target"
FINAL_MAP="$TMP/wifi-connectivity.map"
ELF="$TARGET_DIR/riscv32imfc-unknown-none-elf/release/wifi_connectivity"
EVIDENCE_DIR="${EVIDENCE_DIR:-/private/tmp/ws63-a5b-response-bound-$(date +%Y%m%d-%H%M%S)}"
IDENTITY="$EVIDENCE_DIR/connectivity-artifact.json"
SUMMARY_DIR="$EVIDENCE_DIR/contract"

if [ -d "$EVIDENCE_DIR" ] && [ -n "$(ls -A "$EVIDENCE_DIR")" ]; then
    echo "ERROR: EVIDENCE_DIR must be empty: $EVIDENCE_DIR" >&2
    exit 1
fi
mkdir -p "$EVIDENCE_DIR"

echo "==> build the final public-facade connectivity image"
PLAIN_RUSTFLAGS="-Clink-arg=--no-relax"$'\x1f'"-Clink-arg=-Map=$FINAL_MAP"
(
    cd "$HERE"
    WS63_WIFI_SSID="$WS63_WIFI_SSID" \
    WS63_WIFI_PASSPHRASE="$WS63_WIFI_PASSPHRASE" \
    CARGO_ENCODED_RUSTFLAGS="$PLAIN_RUSTFLAGS" \
    CARGO_TARGET_DIR="$TARGET_DIR" \
        cargo build -Zbuild-std=core,alloc -p wifi_connectivity \
            --release --no-default-features --features "$FEATURES"
)

uv run "$HERE/scripts/check-ws63-plain-rf-elf.py" \
    --elf "$ELF" --require-upstream-supplicant
uv run "$HERE/scripts/check-ws63-supplicant-boundary.py" \
    --map "$FINAL_MAP" --elf "$ELF"
uv run "$HERE/scripts/check-ws63-runtime-compat.py" --elf "$ELF"
uv run "$HERE/hil/ws63-connectivity-reset-matrix.py" \
    --write-artifact-identity "$IDENTITY" \
    --elf "$ELF" \
    --profile-id "$PROFILE_ID"

{
    printf 'parent_commit=%s\n' "$(git -C "$HERE" rev-parse HEAD)"
    printf 'hisi_rf_ws63_commit=%s\n' \
        "$(git -C "$HERE/crates/chips/ws63/hisi-rf-ws63" rev-parse HEAD)"
    printf 'hisi_rf_commit=%s\n' \
        "$(git -C "$HERE/crates/hisi-rf" rev-parse HEAD)"
    printf 'hisi_rf_core_commit=%s\n' \
        "$(git -C "$HERE/crates/hisi-rf-core" rev-parse HEAD)"
    printf 'hisi_rtos_commit=%s\n' \
        "$(git -C "$HERE/crates/hisi-rtos" rev-parse HEAD)"
    printf 'ws63_radio_sys_commit=%s\n' \
        "$(git -C "$HERE/crates/chips/ws63/ws63-radio-sys" rev-parse HEAD)"
} > "$EVIDENCE_DIR/release-closure.txt"

echo "==> flash once through the canonical FlashPlan bin path"
(
    cd "$HERE"
    PROBE_SPEED="$PROBE_SPEED" PORT="" \
        bash hil/cargo-run-hw.sh "$ELF"
)

echo "==> run unchanged-image response-bound connectivity matrix"
uv run "$HERE/hil/ws63-connectivity-reset-matrix.py" \
    --port "$PORT" \
    --runs "$RUNS" \
    --timeout "$TIMEOUT" \
    --stage connectivity \
    --require-contract \
    --require-resource-calibration \
    --max-runner-step-ms "$MAX_RUNNER_STEP_MS" \
    --artifact-identity "$IDENTITY" \
    --elf "$ELF" \
    --profile-id "$PROFILE_ID" \
    --output "$SUMMARY_DIR"

echo "a5b-response-bound: evidence saved at $EVIDENCE_DIR"
echo "WS63 A5B RESPONSE BOUND: PASS"
