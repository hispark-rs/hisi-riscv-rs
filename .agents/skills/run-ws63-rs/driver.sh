#!/usr/bin/env bash
# WS63-RS build driver — checks library compilation, examples, docs, linting.
# Usage: bash driver.sh [check|doc|clippy|fmt|all]

set -euo pipefail
# Default target comes from .cargo/config.toml (riscv32imfc-unknown-none-elf, the
# no-atomic hard-float target baked into the `ws63` toolchain). Override via TARGET=.
TARGET="${TARGET:-riscv32imfc-unknown-none-elf}"
PASS=0
FAIL=0

green() { echo -e "\033[32m  PASS\033[0m $1"; }
red()   { echo -e "\033[31m  FAIL\033[0m $1"; }
banner(){ echo ""; echo "══════ $* ══════"; }

check_step() {
    local desc="$1" cmd="$2"
    echo -n "  $desc ... "
    if eval "$cmd" >/dev/null 2>&1; then
        PASS=$((PASS + 1))
        echo "OK"
    else
        FAIL=$((FAIL + 1))
        echo "FAILED"
    fi
}

# ── Library check ──────────────────────────────────────────────────
run_check() {
    banner "cargo check"
    check_step "hisi-riscv-hal"      "cargo check -p hisi-riscv-hal --no-default-features --features chip-ws63 --target $TARGET"
    check_step "ws63-pac"      "cargo check -p ws63-pac --target $TARGET"
    check_step "hisi-riscv-rt"        "cargo check -p hisi-riscv-rt --target $TARGET"
    check_step "blinky (check)" "cargo check -p blinky --target $TARGET"
    check_step "workspace"      "cargo check --workspace --target $TARGET"

    banner "cargo doc"
    check_step "hisi-riscv-hal docs"  "cargo doc -p hisi-riscv-hal --no-default-features --features chip-ws63 --target $TARGET --no-deps 2>/dev/null"

    banner "blinky release build (links via hisi-riscv-rt linker scripts)"
    # blinky links now (dual-PAC fixed + hisi-riscv-rt exports its linker scripts), so do a
    # real release build, not just check.
    check_step "blinky build"  "cargo build -p blinky --target $TARGET --release"
}

# ── Clippy ─────────────────────────────────────────────────────────
run_clippy() {
    banner "cargo clippy"
    check_step "hisi-riscv-hal clippy" "cargo clippy -p hisi-riscv-hal --no-default-features --features chip-ws63 --target $TARGET -- -D warnings 2>&1 | grep -q 'Finished'"
}

# ── Format ─────────────────────────────────────────────────────────
run_fmt() {
    banner "cargo fmt"
    check_step "formatting" "cargo fmt --all -- --check"
}

# ── All ────────────────────────────────────────────────────────────
run_all() {
    run_check
    echo ""
    run_fmt
    echo ""
    run_clippy
}

# ── Report ─────────────────────────────────────────────────────────
report() {
    banner "Results"
    echo "  $PASS passed, $FAIL failed ($(( PASS + FAIL )) total)"
    if [ "$FAIL" -gt 0 ]; then
        echo "  Some checks FAILED — review output above"
        exit 1
    fi
}

trap report EXIT

case "${1:-all}" in
    check)  run_check ;;
    doc)    run_check ;;  # doc included in check
    clippy) run_clippy ;;
    fmt)    run_fmt ;;
    all)    run_all ;;
    *)      echo "Usage: $0 {check|doc|clippy|fmt|all}"; exit 1 ;;
esac
