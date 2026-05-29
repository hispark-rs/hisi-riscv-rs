#!/usr/bin/env bash
# WS63-RS build driver — checks library compilation, examples, docs, linting.
# Usage: bash driver.sh [check|doc|clippy|fmt|all]

set -euo pipefail
TARGET="${TARGET:-riscv32imafc-unknown-none-elf}"
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
    check_step "ws63-hal"      "cargo check -p ws63-hal --target $TARGET"
    check_step "ws63-pac"      "cargo check -p ws63-pac --target $TARGET"
    check_step "ws63-rt"        "cargo check -p ws63-rt --target $TARGET"
    check_step "blinky (check)" "cargo check -p blinky --target $TARGET"
    check_step "workspace"      "cargo check --target $TARGET"

    banner "cargo doc"
    check_step "ws63-hal docs"  "cargo doc -p ws63-hal --target $TARGET --no-deps 2>/dev/null"

    banner "size check (blinky release check)"
    # Build-check with release profile (actual linking may fail, check-only)
    check_step "blinky release"  "cargo check -p blinky --target $TARGET --release"
}

# ── Clippy ─────────────────────────────────────────────────────────
run_clippy() {
    banner "cargo clippy"
    check_step "ws63-hal clippy" "cargo clippy -p ws63-hal --target $TARGET -- -W warnings 2>&1 | grep -q 'Finished'"
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
