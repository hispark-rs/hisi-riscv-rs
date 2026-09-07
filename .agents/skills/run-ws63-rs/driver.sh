#!/usr/bin/env bash
# Run the parent repository's current WS63 build and host-test contracts.

set -euo pipefail

TARGET="${TARGET:-riscv32imfc-unknown-none-elf}"
HOST_TARGET="${HOST_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
BUILD_STD=(-Zbuild-std=core,alloc)
PASS=0
FAIL=0

banner() {
    printf '\n====== %s ======\n' "$*"
}

run_step() {
    local description="$1"
    shift
    printf '\n-- %s\n' "$description"
    if "$@"; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
    fi
}

run_check() {
    banner "RISC-V checks"
    run_step "WS63 STA/general workspace" \
        cargo check "${BUILD_STD[@]}" --workspace --exclude wifi_softap \
        --features hisi-rf/chip-ws63,hisi-rf/profile-wifi-wpa2-smoltcp \
        --target "$TARGET"
    run_step "WS63 SoftAP archive lane" \
        cargo check "${BUILD_STD[@]}" -p wifi_softap --target "$TARGET"
    run_step "hisi-hal stable WS63 surface" \
        cargo check "${BUILD_STD[@]}" -p hisi-hal --no-default-features \
        --features chip-ws63,rt --target "$TARGET"
    run_step "blinky release link" \
        cargo build "${BUILD_STD[@]}" -p blinky --release --target "$TARGET"
}

run_test() {
    banner "Host tests"
    run_step "hisi-hal host tests" \
        cargo test -p hisi-hal --no-default-features --features chip-ws63 \
        --target "$HOST_TARGET"
    run_step "transitional RF host tests" \
        cargo test -p ws63-rf-rs --lib --target "$HOST_TARGET"
    run_step "hisi-rtos host and UI tests" \
        cargo test -p hisi-rtos --target "$HOST_TARGET"
}

run_doc() {
    banner "Rustdoc"
    run_step "WS63 public API docs" \
        cargo doc "${BUILD_STD[@]}" -p hisi-hal -p ws63-pac -p hisi-riscv-rt \
        --features hisi-hal/chip-ws63 --target "$TARGET" --no-deps
}

run_clippy() {
    banner "Clippy"
    run_step "WS63 STA/general workspace clippy" \
        cargo clippy "${BUILD_STD[@]}" --workspace --exclude wifi_softap \
        --features hisi-rf/chip-ws63,hisi-rf/profile-wifi-wpa2-smoltcp \
        --target "$TARGET" -- -D warnings
    run_step "WS63 SoftAP clippy" \
        cargo clippy "${BUILD_STD[@]}" -p wifi_softap --target "$TARGET" -- -D warnings
}

run_fmt() {
    banner "Formatting"
    run_step "workspace format" cargo fmt --all -- --check
}

report() {
    banner "Results"
    printf '%d passed, %d failed\n' "$PASS" "$FAIL"
    if test "$FAIL" -ne 0; then
        trap - EXIT
        exit 1
    fi
}

trap report EXIT

case "${1:-all}" in
    check) run_check ;;
    test) run_test ;;
    doc) run_doc ;;
    clippy) run_clippy ;;
    fmt) run_fmt ;;
    all)
        run_check
        run_test
        run_doc
        run_fmt
        run_clippy
        ;;
    *)
        echo "Usage: $0 {check|test|doc|clippy|fmt|all}"
        exit 2
        ;;
esac
