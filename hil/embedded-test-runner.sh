#!/usr/bin/env bash
# Cargo *runner* for on-target HIL tests (the `tests-hil` crate). Turns
# `cargo test --target riscv32imfc-unknown-none-elf` into "run each embedded-test
# case on a real WS63 board over semihosting" instead of "boot in QEMU".
#
# Cargo invokes a test runner as `<runner> <path-to-built-test-ELF> [args...]`
# where the trailing args are the libtest/embedded-test args (e.g. `--list`, a
# test-name filter, `--exact`). This script:
#   1. fills the WS63 boot-header body SHA-256 in place via `hisi-fwpkg patch-hash`
#      (the ELF already carries the 0x300 header because tests-hil builds
#      hisi-riscv-rt with the `boot-header` feature), then
#   2. hands the ELF straight to the patched probe-rs fork's `probe-rs run`,
#      which detects the `.embedded_test` section, flashes/boots the ELF, and
#      drives each test in turn over semihosting (SYS_GET_CMDLINE / SYS_EXIT),
#      reporting libtest-compatible results back to `cargo test`.
#
# Enable it for the test invocation only (does NOT touch the QEMU `cargo run`
# default in .cargo/config.toml):
#   CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/embedded-test-runner.sh \
#       cargo test -p tests-hil --target riscv32imfc-unknown-none-elf
#
# Env (all optional, sensible defaults — mirrors hil/cargo-run-hw.sh):
#   PROBE_RS     probe-rs binary                 (default: `probe-rs` in PATH; needs the
#                                                 patched fork hispark-rs/probe-rs,
#                                                 branch add-hisilicon-ws63-bs21)
#   PROBE_CHIP   probe-rs --chip value           (default WS63)
#   PROBE_YAML   --chip-description-path YAML     (default: empty = built-in DB)
#   HISI_FWPKG   hisi-fwpkg binary               (default: `hisi-fwpkg` in PATH)
set -euo pipefail

ELF="${1:?cargo passes the built test ELF path as \$1}"
shift # remaining args ($@) are the embedded-test/libtest args, forwarded verbatim

PROBE_RS="${PROBE_RS:-probe-rs}"
PROBE_CHIP="${PROBE_CHIP:-WS63}"
HISI_FWPKG="${HISI_FWPKG:-hisi-fwpkg}"

command -v "$HISI_FWPKG" >/dev/null 2>&1 || {
    echo "embedded-test-runner: '$HISI_FWPKG' not found — install hisi-fwpkg (https://github.com/hispark-rs/hisi-fwpkg)." >&2
    exit 1
}
command -v "$PROBE_RS" >/dev/null 2>&1 || {
    echo "embedded-test-runner: '$PROBE_RS' not found — needs the patched fork (hispark-rs/probe-rs, branch add-hisilicon-ws63-bs21)." >&2
    exit 1
}

yaml_args=()
[ -n "${PROBE_YAML:-}" ] && yaml_args=(--chip-description-path "$PROBE_YAML")

echo "embedded-test-runner: patch-hashing $(basename "$ELF") (fills the boot-header body SHA-256 in place)" >&2
"$HISI_FWPKG" patch-hash "$ELF"

echo "embedded-test-runner: probe-rs run --chip $PROBE_CHIP $(basename "$ELF") (semihosting embedded-test harness)" >&2
exec "$PROBE_RS" run --chip "$PROBE_CHIP" "${yaml_args[@]}" "$ELF" "$@"
