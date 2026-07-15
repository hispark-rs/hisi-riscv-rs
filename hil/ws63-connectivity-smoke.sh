#!/usr/bin/env bash
# Build, flash, and verify the WS63 A4 Wi-Fi vertical slice on real silicon.
#
# This is deliberately separate from hil-smoke.sh: it requires a controlled
# Personal-mode AP credentials, J-Link nRST, and a longer UART observation
# window. The profile explicitly selects either the vendor oracle or the pinned
# upstream hostap source path.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WPA_TAG="wpa2-personal-2026-07-13"
WPA_ASSET="libwpa_supplicant_wpa2_personal.a"
WPA_SHA256="891c195279d768ce5664e16fecac95f353fd2217c4e3590315baa4cb6e7f25a2"
WPA_URL="https://github.com/hispark-rs/ws63-RF/releases/download/$WPA_TAG/$WPA_ASSET"
MONITOR="${MONITOR:-60}"
PROBE_SPEED="${PROBE_SPEED:-1000}"
PORT="${PORT:-}"
PYTHON="${PYTHON:-}"
PROFILE="${WS63_CONNECTIVITY_PROFILE:-vendor-wpa2}"

usage() {
    cat <<'EOF'
Usage: PORT=/dev/ttyUSB0 WS63_WIFI_SSID=... WS63_WIFI_PASSPHRASE=... \
       hil/ws63-connectivity-smoke.sh
       hil/ws63-connectivity-smoke.sh --preflight

Optional: WS63_WPA_ARCHIVE, PROBE_RS, PROBE_YAML, PROBE_CHIP, PROBE_SPEED,
          HISI_FWPKG, UART_BAUD, MONITOR,
          WS63_CONNECTIVITY_PROFILE={vendor-wpa2|upstream-wpa2|upstream-wpa3},
          WS63_WIFI_AP_MODE={pure-wpa3|transition} for upstream-wpa3.
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

resolve_python() {
    local python="$PYTHON"
    if [ -z "$python" ]; then
        python="$(uv python find '>=3.11')" || {
            echo "ERROR: uv could not provide Python >=3.11" >&2
            return 1
        }
    fi
    "$python" -c 'import tomllib' >/dev/null 2>&1 || {
        echo "ERROR: RF post-link tools require Python >=3.11 with tomllib: $python" >&2
        return 1
    }
    printf '%s\n' "$python"
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
    local failed=0 archive actual python
    case "$PROFILE" in
        vendor-wpa2|upstream-wpa2|upstream-wpa3) ;;
        *)
            echo "ERROR: unsupported WS63_CONNECTIVITY_PROFILE: $PROFILE" >&2
            failed=1
            ;;
    esac
    for command in cargo JLinkExe "${PROBE_RS:-probe-rs}" "${HISI_FWPKG:-hisi-fwpkg}" uv riscv64-unknown-elf-gcc; do
        require_command "$command" || failed=1
    done
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
    if python="$(resolve_python)"; then
        PYTHON="$python"
        export PYTHON
    else
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
    echo "connectivity-smoke: preflight PASS (profile=$PROFILE, probe=${PROBE_SPEED}kHz, monitor=${MONITOR}s)"
}

assert_marker() {
    local pattern="$1" description="$2"
    if grep -qE "$pattern" "$LOG"; then
        echo "  PASS $description"
    else
        echo "  FAIL $description (missing: $pattern)" >&2
        fail=1
    fi
}

assert_absent() {
    local pattern="$1" description="$2"
    if grep -qE "$pattern" "$LOG"; then
        echo "  FAIL $description (unexpected: $pattern)" >&2
        fail=1
    else
        echo "  PASS $description"
    fi
}

case "${1:-}" in
    --preflight) preflight; exit $? ;;
    -h|--help) usage; exit 0 ;;
    "") ;;
    *) usage >&2; exit 2 ;;
esac

preflight
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
LOG="$TMP/uart.log"
TARGET_DIR="$TMP/target"
ELF="$TARGET_DIR/riscv32imfc-unknown-none-elf/release/wifi_init_smoke"
ARCHIVE=""
FEATURES=""
case "$PROFILE" in
    vendor-wpa2)
        ARCHIVE="$(resolve_archive)"
        FEATURES="full-init,wpa"
        ;;
    upstream-wpa2)
        FEATURES="full-init,upstream-supplicant"
        ;;
    upstream-wpa3)
        FEATURES="full-init,upstream-supplicant,upstream-wpa3"
        ;;
esac

echo "==> guarded connectivity link (profile=$PROFILE; credentials use an ephemeral target dir)"
(
    cd "$HERE"
    WS63_WPA_ARCHIVE="$ARCHIVE" \
    WS63_WIFI_SSID="$WS63_WIFI_SSID" \
    WS63_WIFI_PASSPHRASE="$WS63_WIFI_PASSPHRASE" \
    WS63_RF_FEATURES="$FEATURES" \
    CARGO_TARGET_DIR="$TARGET_DIR" \
        bash chips/ws63/rf/tools/rf-build-full-init-lld-layout-patch.sh
)

echo "==> planned-bin download, J-Link nRST, and UART capture"
(
    cd "$HERE"
    PORT="$PORT" MONITOR="$MONITOR" PROBE_SPEED="$PROBE_SPEED" \
        bash hil/cargo-run-hw.sh "$ELF"
) 2>&1 | tee "$LOG"

fail=0
assert_marker 'RF2_INIT_OK ifname=hisi-rf' 'chip-neutral radio initialized'
assert_marker 'A4_RADIO_EVENT kind=initialized' 'initialized event delivered'
assert_marker 'A4_RADIO_EVENT kind=scan-completed' 'scan event delivered'
assert_marker 'A4_RADIO_EVENT kind=connected' 'connect event delivered'
case "$PROFILE" in
    vendor-wpa2)
        assert_marker 'RF5B_WPA_CONNECT_OK' 'vendor-oracle WPA2 association completed'
        ;;
    upstream-wpa2)
        assert_marker 'W2D_WPA2_CONNECT_OK' 'upstream hostap WPA2 association completed'
        ;;
    upstream-wpa3)
        assert_marker "W2E_AP_SECURITY mode=${WS63_WIFI_AP_MODE}" 'scan RSNE matches the controlled WPA3 AP mode'
        assert_marker 'W2E_WPA3_CONNECT_OK pmf=required' 'upstream hostap SAE association completed with required PMF'
        ;;
esac
assert_marker 'RF5A_DHCP_OK addr=' 'DHCP lease acquired'
assert_marker 'RF5A_ARP_OK mode=smoltcp-neighbor-cache' 'neighbor cache resolved L2 peer'
assert_marker 'RF5C_PING_OK target=1\.1\.1\.1 .*rx=0x0*[1-9a-fA-F]' 'public ICMP received replies'
assert_marker 'RF5C_CONNECTIVITY_SUMMARY .*rx_queue_drop=0x0+([^0-9a-fA-F]|$)' 'bounded RX queue had no drops'
assert_marker 'A4_NET_RUNNER_STEADY' 'long-lived network runner entered steady state'
assert_marker 'A4_DHCP_RENEW_OK client=0x0*[1-9a-fA-F].*server=0x0*[1-9a-fA-F]' 'DHCP renewal request and response observed'
assert_absent 'A4_NET_ERR|RF5A_DHCP_TIMEOUT|RF5B_.*ERR|W2D_.*ERR|W2E_.*ERR|panicked at' 'no fatal connectivity marker'

if [ "$fail" -ne 0 ]; then
    echo "WS63 CONNECTIVITY SMOKE: FAIL" >&2
    echo "--- UART tail ---" >&2
    tail -80 "$LOG" >&2
    exit 1
fi

echo "WS63 CONNECTIVITY SMOKE: PASS"
