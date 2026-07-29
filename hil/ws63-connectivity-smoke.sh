#!/usr/bin/env bash
# Build, flash, and verify the WS63 A4 Wi-Fi vertical slice on real silicon.
#
# This is deliberately separate from hil-smoke.sh: it requires a controlled
# Personal-mode AP credentials, J-Link nRST, and a longer UART observation
# window. The profile explicitly selects either the vendor oracle or the pinned
# upstream hostap source path.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/ws63-wifi-credentials.sh
source "$HERE/hil/lib/ws63-wifi-credentials.sh"

WPA_TAG="wpa2-personal-2026-07-13"
WPA_ASSET="libwpa_supplicant_wpa2_personal.a"
WPA_SHA256="891c195279d768ce5664e16fecac95f353fd2217c4e3590315baa4cb6e7f25a2"
WPA_URL="https://github.com/hispark-rs/ws63-RF/releases/download/$WPA_TAG/$WPA_ASSET"
PROBE_SPEED="${PROBE_SPEED:-3000}"
PORT="${PORT:-}"
PROFILE="${WS63_CONNECTIVITY_PROFILE:-upstream-wpa2}"
EXPECT="${WS63_CONNECTIVITY_EXPECT:-full}"
CRYPTO_CONTENTION_HIL="${WS63_CRYPTO_CONTENTION_HIL:-0}"
if [ "$EXPECT" = init-scan ]; then
    MONITOR="${MONITOR:-35}"
    WS63_WIFI_SSID="${WS63_WIFI_SSID:-ci-fixture}"
    WS63_WIFI_PASSPHRASE="${WS63_WIFI_PASSPHRASE:-fixture-passphrase}"
    if [ "$PROFILE" = upstream-wpa3 ]; then
        WS63_WIFI_AP_MODE="${WS63_WIFI_AP_MODE:-transition}"
    fi
else
    MONITOR="${MONITOR:-60}"
fi

usage() {
    cat <<'EOF'
Usage: PORT=/dev/ttyUSB0 WS63_WIFI_SSID=... WS63_WIFI_PASSPHRASE=... \
       hil/ws63-connectivity-smoke.sh
       WS63_WIFI_ENV_FILE=/path/to/local-0600.env \
       PORT=/dev/ttyUSB0 hil/ws63-connectivity-smoke.sh
       hil/ws63-connectivity-smoke.sh --preflight

Optional: WS63_WPA_ARCHIVE, PROBE_RS, PROBE_YAML, PROBE_CHIP, PROBE_SPEED,
          HISI_FWPKG, UART_BAUD, MONITOR,
          EVIDENCE_DIR (default: timestamped directory under /private/tmp),
          WS63_CONNECTIVITY_PROFILE={upstream-wpa2|upstream-wpa3|vendor-wpa2},
          WS63_CONNECTIVITY_EXPECT={full|init-scan}; init-scan uses public
          fixture credentials and proves only image/startup/RF init/scan/runner,
          WS63_WIFI_AP_MODE={pure-wpa3|transition} for upstream-wpa3,
          WS63_CRYPTO_CONTENTION_HIL=1 for the diagnostic two-task mutex gate.
          WS63_WIFI_ENV_FILE is a local-only, non-symlink 0600 file containing
          exactly WS63_WIFI_SSID=... and WS63_WIFI_PASSPHRASE=...; it is parsed
          without shell evaluation and retained by default. Set
          WS63_WIFI_ENV_FILE_DISPOSITION=delete for a one-shot file.
EOF
}

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "ERROR: required command not found: $1" >&2
        return 1
    }
}

resolve_archive() {
    if [ -n "${WS63_WPA_ARCHIVE:-}" ]; then
        test -f "$WS63_WPA_ARCHIVE" || {
            echo "ERROR: WS63_WPA_ARCHIVE not found: $WS63_WPA_ARCHIVE" >&2
            return 1
        }
        printf '%s\n' "$WS63_WPA_ARCHIVE"
        return
    fi

    local cache="${XDG_CACHE_HOME:-$HOME/.cache}/hispark-rs/ws63-rf/$WPA_TAG"
    local archive="$cache/$WPA_ASSET"
    mkdir -p "$cache"
    if [ ! -f "$archive" ] || [ "$(sha256_file "$archive")" != "$WPA_SHA256" ]; then
        require_command curl
        echo "connectivity-smoke: fetching pinned WPA2 archive" >&2
        curl --fail --location --retry 3 --output "$archive.tmp" "$WPA_URL"
        mv "$archive.tmp" "$archive"
    fi
    printf '%s\n' "$archive"
}

preflight() {
    local failed=0 archive actual
    case "$PROFILE" in
        vendor-wpa2|upstream-wpa2|upstream-wpa3) ;;
        *)
            echo "ERROR: unsupported WS63_CONNECTIVITY_PROFILE: $PROFILE" >&2
            failed=1
            ;;
    esac
    case "$EXPECT" in
        full|init-scan) ;;
        *)
            echo "ERROR: WS63_CONNECTIVITY_EXPECT must be full or init-scan" >&2
            failed=1
            ;;
    esac
    case "$CRYPTO_CONTENTION_HIL" in
        0|1) ;;
        *)
            echo "ERROR: WS63_CRYPTO_CONTENTION_HIL must be 0 or 1" >&2
            failed=1
            ;;
    esac
    if [ "$CRYPTO_CONTENTION_HIL" = 1 ] && [ "$PROFILE" != upstream-wpa3 ]; then
        echo "ERROR: crypto contention HIL currently requires upstream-wpa3" >&2
        failed=1
    fi
    if [ "$CRYPTO_CONTENTION_HIL" = 1 ] && [ "$EXPECT" != full ]; then
        echo "ERROR: crypto contention HIL requires WS63_CONNECTIVITY_EXPECT=full" >&2
        failed=1
    fi
    for command in cargo JLinkExe "${PROBE_RS:-probe-rs}" "${HISI_FWPKG:-hisi-fwpkg}" uv; do
        require_command "$command" || failed=1
    done
    if [ "$PROFILE" = vendor-wpa2 ]; then
        require_command riscv64-unknown-elf-gcc || failed=1
    fi
    if [ -z "$PORT" ]; then
        echo "ERROR: set PORT to the WS63 UART0 device" >&2
        failed=1
    elif [ ! -e "$PORT" ]; then
        echo "ERROR: UART device does not exist: $PORT" >&2
        failed=1
    fi
    if [ -z "${WS63_WIFI_PASSPHRASE:-}" ]; then
        echo "ERROR: set WS63_WIFI_PASSPHRASE through the HIL secret store" >&2
        failed=1
    fi
    if [ -z "${WS63_WIFI_SSID:-}" ]; then
        echo "ERROR: set WS63_WIFI_SSID to the controlled AP" >&2
        failed=1
    fi
    if [ "$PROFILE" = upstream-wpa3 ] &&
        ! [[ "${WS63_WIFI_AP_MODE:-}" =~ ^(pure-wpa3|transition)$ ]]; then
        echo "ERROR: upstream-wpa3 requires WS63_WIFI_AP_MODE=pure-wpa3 or transition" >&2
        failed=1
    fi
    if [ -n "${PROBE_YAML:-}" ] && [ ! -f "$PROBE_YAML" ]; then
        echo "ERROR: PROBE_YAML not found: $PROBE_YAML" >&2
        failed=1
    fi
    if [ "$failed" -eq 0 ] && [ "$PROFILE" = vendor-wpa2 ]; then
        require_command curl || failed=1
        archive="$(resolve_archive)" || failed=1
        if [ "$failed" -eq 0 ]; then
            actual="$(sha256_file "$archive")"
            if [ "$actual" != "$WPA_SHA256" ]; then
                echo "ERROR: WPA2 archive hash mismatch: $actual" >&2
                failed=1
            else
                echo "connectivity-smoke: archive verified: $WPA_SHA256"
            fi
        fi
    fi
    [ "$failed" -eq 0 ] || return 1
    echo "connectivity-smoke: preflight PASS (profile=$PROFILE, expect=$EXPECT, probe=${PROBE_SPEED}kHz, monitor=${MONITOR}s)"
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
EVIDENCE_DIR="${EVIDENCE_DIR:-/private/tmp/ws63-connectivity-smoke-$(date +%Y%m%d-%H%M%S)}"
if [ -d "$EVIDENCE_DIR" ] && [ -n "$(ls -A "$EVIDENCE_DIR")" ]; then
    echo "ERROR: EVIDENCE_DIR must be empty: $EVIDENCE_DIR" >&2
    exit 1
fi
mkdir -p "$EVIDENCE_DIR"
LOG="$EVIDENCE_DIR/run-01.uart.log"
TARGET_DIR="$TMP/target"
FINAL_MAP="$TMP/wifi-init-smoke-rf-lld-final.map"
ELF="$TARGET_DIR/riscv32imfc-unknown-none-elf/release/wifi_init_smoke"
IDENTITY="$EVIDENCE_DIR/connectivity-artifact.json"
ARCHIVE=""
FEATURES=""
USE_FINAL_CONNECTIVITY=0
if { [ "$PROFILE" = upstream-wpa2 ] || [ "$PROFILE" = upstream-wpa3 ]; } &&
    [ "$EXPECT" = full ] && [ "$CRYPTO_CONTENTION_HIL" = 0 ]; then
    USE_FINAL_CONNECTIVITY=1
    FINAL_MAP="$TMP/wifi-connectivity.map"
    ELF="$TARGET_DIR/riscv32imfc-unknown-none-elf/release/wifi_connectivity"
fi
case "$PROFILE" in
    vendor-wpa2)
        ARCHIVE="$(resolve_archive)"
        FEATURES="full-init,wpa"
        ;;
    upstream-wpa2)
        if [ "$USE_FINAL_CONNECTIVITY" = 1 ]; then
            FEATURES="wpa2"
        else
            FEATURES="full-init,upstream-supplicant"
        fi
        ;;
    upstream-wpa3)
        if [ "$USE_FINAL_CONNECTIVITY" = 1 ]; then
            FEATURES="wpa3"
        else
            FEATURES="full-init,upstream-supplicant,upstream-wpa3"
        fi
        ;;
esac
if [ "$CRYPTO_CONTENTION_HIL" = 1 ]; then
    FEATURES="$FEATURES,rf-crypto-contention-diag"
fi

case "$PROFILE" in
    vendor-wpa2)
        echo "==> guarded vendor-oracle link (credentials use an ephemeral target dir)"
        (
            cd "$HERE"
            WS63_WPA_ARCHIVE="$ARCHIVE" \
            WS63_WIFI_SSID="$WS63_WIFI_SSID" \
            WS63_WIFI_PASSPHRASE="$WS63_WIFI_PASSPHRASE" \
            WS63_RF_FEATURES="$FEATURES" \
            WS63_RF_FINAL_MAP="$FINAL_MAP" \
            CARGO_TARGET_DIR="$TARGET_DIR" \
                bash chips/ws63/rf/tools/rf-build-full-init-lld-layout-patch.sh
        )
        ;;
    upstream-wpa2|upstream-wpa3)
        echo "==> plain Cargo connectivity link (profile=$PROFILE; credentials use an ephemeral target dir)"
        PLAIN_RUSTFLAGS="-Clink-arg=--no-relax"$'\x1f'"-Clink-arg=-Map=$FINAL_MAP"
        if [ "$USE_FINAL_CONNECTIVITY" = 1 ]; then
            (
                cd "$HERE"
                WS63_WIFI_SSID="$WS63_WIFI_SSID" \
                WS63_WIFI_PASSPHRASE="$WS63_WIFI_PASSPHRASE" \
                CARGO_ENCODED_RUSTFLAGS="$PLAIN_RUSTFLAGS" \
                CARGO_TARGET_DIR="$TARGET_DIR" \
                    cargo build -Zbuild-std=core,alloc -p wifi_connectivity \
                        --release --no-default-features --features "$FEATURES"
            )
        else
            (
                cd "$HERE"
                WS63_WIFI_SSID="$WS63_WIFI_SSID" \
                WS63_WIFI_PASSPHRASE="$WS63_WIFI_PASSPHRASE" \
                CARGO_ENCODED_RUSTFLAGS="$PLAIN_RUSTFLAGS" \
                CARGO_TARGET_DIR="$TARGET_DIR" \
                    cargo build -Zbuild-std=core,alloc -p wifi_init_smoke \
                        --release --features "$FEATURES"
            )
        fi
        uv run "$HERE/scripts/check-ws63-plain-rf-elf.py" \
            --elf "$ELF" --require-upstream-supplicant
        uv run "$HERE/scripts/check-ws63-supplicant-boundary.py" \
            --map "$FINAL_MAP" --elf "$ELF"
        ;;
esac

uv run "$HERE/hil/ws63-connectivity-reset-matrix.py" \
    --write-artifact-identity "$IDENTITY" \
    --elf "$ELF" \
    --profile-id "$PROFILE"

echo "==> planned-bin download, J-Link nRST, and UART capture"
if [ "$PROFILE" = upstream-wpa2 ] || [ "$PROFILE" = upstream-wpa3 ]; then
    uv run "$HERE/scripts/check-ws63-runtime-compat.py" --elf "$ELF"
fi
(
    cd "$HERE"
    PORT="$PORT" MONITOR="$MONITOR" PROBE_SPEED="$PROBE_SPEED" \
        bash hil/cargo-run-hw.sh "$ELF"
) 2>&1 | tee "$LOG"

CONTRACT_OUTPUT="$EVIDENCE_DIR/contract"
CONTRACT_STAGE=connectivity
if [ "$EXPECT" = init-scan ]; then
    CONTRACT_STAGE=init-scan
fi
contract_args=(
    --analyze-dir "$EVIDENCE_DIR"
    --output "$CONTRACT_OUTPUT"
    --stage "$CONTRACT_STAGE"
    --require-contract
    --artifact-identity "$IDENTITY"
    --elf "$ELF"
    --profile-id "$PROFILE"
)
if [ "$PROFILE" = upstream-wpa3 ] && [ "$EXPECT" = full ]; then
    contract_args+=(--required-ap-mode "$WS63_WIFI_AP_MODE")
fi
if ! uv run "$HERE/hil/ws63-connectivity-reset-matrix.py" "${contract_args[@]}"; then
    echo "WS63 CONNECTIVITY CONTRACT: FAIL" >&2
    echo "--- UART tail ---" >&2
    tail -80 "$LOG" >&2
    exit 1
fi

if [ "$PROFILE" = upstream-wpa2 ] || [ "$PROFILE" = upstream-wpa3 ]; then
    if ! grep -q 'W2D_NATIVE_RUNNER_RX_READY' "$LOG"; then
        echo "WS63 UPSTREAM RUNNER CONTRACT: FAIL" >&2
        tail -80 "$LOG" >&2
        exit 1
    fi
fi

if [ "$CRYPTO_CONTENTION_HIL" = 1 ]; then
    if ! grep -qE 'RFDBG_HW_CONTENTION tests=0x0*1 failures=0x0+ observed=0x0*1 holder=0x0*1 waiter=0x0*1' "$LOG"; then
        echo "WS63 CRYPTO CONTENTION CONTRACT: FAIL" >&2
        tail -80 "$LOG" >&2
        exit 1
    fi
fi

if [ "$EXPECT" = init-scan ]; then
    echo "connectivity-smoke: evidence saved at $EVIDENCE_DIR"
    echo "WS63 RADIO INIT/SCAN SMOKE: PASS"
    exit 0
fi
echo "connectivity-smoke: evidence saved at $EVIDENCE_DIR"
echo "WS63 CONNECTIVITY SMOKE: PASS"
