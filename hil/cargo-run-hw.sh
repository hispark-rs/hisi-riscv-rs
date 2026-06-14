#!/usr/bin/env bash
# Cargo *runner* that flashes a freshly-built ELF to a real WS63 board and boots
# it — turning `cargo run` into "flash to hardware" instead of "boot in QEMU".
#
# Cargo invokes a runner as `<runner> <path-to-built-elf> [args...]`, so this
# script takes the ELF as $1. With the `boot-header` feature the 0x300 HiSilicon
# header is already baked into the ELF at link time, so the bare ELF is directly
# bootable — there is no `hisi-fwpkg image` step and no intermediate .img. We
# only fill the body SHA-256 in place with `hisi-fwpkg patch-hash` (flashboot
# checks the hash even with secure-verify off — secure-off skips the ECC
# signature, not the hash), then `probe-rs download` the patched ELF straight to
# flash with the patched probe-rs fork, reset the chip, and (if PORT is set)
# stream UART0 so you see the output.
#
# Enable it for one run via the per-target runner env var, or use `just run-hw`:
#   CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/cargo-run-hw.sh \
#       cargo run -p blinky --release
#
# Env (all optional, sensible defaults):
#   PROBE_RS     probe-rs binary                    (default: `probe-rs` in PATH)
#   PROBE_CHIP   probe-rs --chip value              (default WS63)
#   PROBE_YAML   --chip-description-path YAML       (default: empty = built-in DB)
#   HISI_FWPKG   hisi-fwpkg binary                  (default: `hisi-fwpkg` in PATH)
#   PORT         board UART0 to stream after reset  (default: none = don't stream)
#   UART_BAUD    UART baud for streaming            (default 115200)
#   MONITOR      seconds to stream UART             (default 10)
set -euo pipefail

ELF="${1:?cargo passes the built ELF path as \$1}"

PROBE_RS="${PROBE_RS:-probe-rs}"
PROBE_CHIP="${PROBE_CHIP:-WS63}"
HISI_FWPKG="${HISI_FWPKG:-hisi-fwpkg}"
UART_BAUD="${UART_BAUD:-115200}"
MONITOR="${MONITOR:-10}"

command -v "$HISI_FWPKG" >/dev/null 2>&1 || {
    echo "run-hw: '$HISI_FWPKG' not found — install hisi-fwpkg (https://github.com/hispark-rs/hisi-fwpkg)." >&2
    exit 1
}
command -v "$PROBE_RS" >/dev/null 2>&1 || {
    echo "run-hw: '$PROBE_RS' not found — needs the patched fork (hispark-rs/probe-rs, branch add-hisilicon-ws63-bs21)." >&2
    exit 1
}

yaml_args=()
[ -n "${PROBE_YAML:-}" ] && yaml_args=(--chip-description-path "$PROBE_YAML")

echo "run-hw: filling body SHA-256 in place: $(basename "$ELF") (0x300 header already baked in)"
"$HISI_FWPKG" patch-hash "$ELF"

echo "run-hw: downloading the patched ELF to flash via probe-rs"
"$PROBE_RS" download --chip "$PROBE_CHIP" "${yaml_args[@]}" "$ELF"

# Optionally start UART capture *before* reset so we don't miss the boot banner.
cap=""
if [ -n "${PORT:-}" ]; then
    stty -F "$PORT" "$UART_BAUD" raw -echo 2>/dev/null || true
    cap="$(mktemp)"
    ( timeout "$MONITOR" cat "$PORT" > "$cap" 2>/dev/null ) &
    cap_pid=$!
    sleep 0.3
fi

echo "run-hw: resetting chip to boot the app"
"$PROBE_RS" reset --chip "$PROBE_CHIP" "${yaml_args[@]}"

if [ -n "${PORT:-}" ]; then
    echo "run-hw: streaming $PORT @ ${UART_BAUD} 8N1 for ${MONITOR}s (Ctrl-C to stop)"
    wait "$cap_pid" 2>/dev/null || true
    tr -d '\0' < "$cap"
    rm -f "$cap"
fi

echo "run-hw: done."
