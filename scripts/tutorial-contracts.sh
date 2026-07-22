#!/usr/bin/env bash
# Executable source of truth for docs/src/tutorials/** command snippets.
#
# The mdBook preprocessor renders blocks marked with:
#   # docs:start <snippet-id>
#   # docs:end
#
# Keep tutorial commands here, then let CI execute the same workflow with
# --no-hardware. Hardware-only snippets are documented here but not executed by
# GitHub-hosted CI.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NO_HARDWARE=0
CHIPS=()

while [ "$#" -gt 0 ]; do
    arg="$1"
    case "$arg" in
        --no-hardware) NO_HARDWARE=1 ;;
        --chip)
            [ "$#" -ge 2 ] || { echo "tutorial-contracts: --chip needs a value" >&2; exit 2; }
            CHIPS+=("$2")
            shift
            ;;
        --chip=*)
            CHIPS+=("${arg#--chip=}")
            ;;
        *)
            echo "usage: $0 [--no-hardware] [--chip ws63|bs21|bs20]" >&2
            exit 2
            ;;
    esac
    shift
done

if [ "${#CHIPS[@]}" -eq 0 ]; then
    CHIPS=(ws63 bs21 bs20)
fi

if [ "$NO_HARDWARE" -ne 1 ]; then
    echo "tutorial-contracts: only --no-hardware is supported here; use HIL for board runs" >&2
    exit 2
fi

HISI_FWPKG="${HISI_FWPKG:-hisi-fwpkg}"
TMPDIR="${TMPDIR:-/tmp}"
WORK="$(mktemp -d "$TMPDIR/hisi-tutorial-contracts.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

need() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "tutorial-contracts: missing required command: $1" >&2
        exit 127
    }
}

need cargo
need cargo-generate
need just
need uv
need "$HISI_FWPKG"

: <<'DOCS_SNIPPETS'
# docs:start app_setup_rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# docs:end

# docs:start app_setup_toolchain
rustup toolchain install nightly-2026-07-09 \
    --profile minimal \
    --component rust-src \
    --component clippy \
    --component rustfmt \
    --component llvm-tools-preview
# docs:end

# docs:start app_setup_check_toolchain
rustc +nightly-2026-07-09 --print target-list | grep riscv32imfc
rustup target list --toolchain nightly-2026-07-09 | grep riscv32imfc || \
    echo "rustup has no prebuilt rust-std yet; use -Zbuild-std=core,alloc"
# docs:end

# docs:start install_cargo_generate_just
cargo install cargo-generate just
# docs:end

# docs:start install_hisi_fwpkg
cargo +stable install hisi-fwpkg-cli --version 0.3.2
# docs:end

# docs:start install_probe_rs
cargo install --git https://github.com/hispark-rs/probe-rs \
    --branch add-hisilicon-ws63-bs21-hil-baseline probe-rs-tools
# docs:end

# docs:start app_setup_qemu
git clone https://github.com/hispark-rs/hisi-riscv-qemu && cd hisi-riscv-qemu
./scripts/build.sh
# docs:end

# docs:start app_setup_check_qemu
qemu-system-riscv32 -M help | grep ws63
# docs:end

# docs:start app_setup_check_target
rustc --print target-list | grep riscv32imfc
# docs:end

# docs:start app_first_generate
cargo generate --git https://github.com/hispark-rs/hisi-rs-template
# docs:end

# docs:start app_first_cd
cd my-blinky
# docs:end

# docs:start app_first_run
just run
# docs:end

# docs:start app_first_flash
just flash
# docs:end

# docs:start app_first_run_hw
just run-hw PORT=/dev/ttyUSB0
# docs:end

# docs:start app_uart_generate
cargo generate --git https://github.com/hispark-rs/hisi-rs-template
# docs:end

# docs:start app_uart_cd
cd my-uart
# docs:end

# docs:start app_uart_run
just run
# docs:end

# docs:start contrib_setup_rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# docs:end

# docs:start contrib_setup_toolchain
rustup toolchain install nightly-2026-07-09 \
    --profile minimal \
    --component rust-src \
    --component clippy \
    --component rustfmt \
    --component llvm-tools-preview
# docs:end

# docs:start contrib_setup_check_toolchain
rustc +nightly-2026-07-09 --print target-list | grep riscv32imfc
rustup target list --toolchain nightly-2026-07-09 | grep riscv32imfc || \
    echo "rustup has no prebuilt rust-std yet; use -Zbuild-std=core,alloc"
# docs:end

# docs:start contrib_clone_repo
git clone --recurse-submodules https://github.com/hispark-rs/hisi-riscv-rs.git
cd hisi-riscv-rs
# docs:end

# docs:start contrib_setup_qemu
cd ..
git clone https://github.com/hispark-rs/hisi-riscv-qemu.git
cd hisi-riscv-qemu
bash scripts/build.sh
# docs:end

# docs:start contrib_setup_check_qemu
./build/qemu-system-riscv32 -M help | grep ws63
# docs:end

# docs:start contrib_install_flash_tools
# hisi-fwpkg
cargo +stable install hisi-fwpkg-cli --version 0.3.2

# 打过补丁的 probe-rs 分支
cargo install --git https://github.com/hispark-rs/probe-rs --branch add-hisilicon-ws63-bs21-hil-baseline probe-rs-tools
# docs:end

# docs:start contrib_check_flash_tools
hisi-fwpkg --help
probe-rs --version
# docs:end

# docs:start contrib_build_blinky
cd ../hisi-riscv-rs
cargo build -Zbuild-std=core,alloc -p blinky --release
# docs:end

# docs:start contrib_ls_blinky
ls target/riscv32imfc-unknown-none-elf/release/blinky
# docs:end

# docs:start contrib_example_template
cargo build -Zbuild-std=core,alloc -p <name> --release
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/<name>
# docs:end

# docs:start contrib_example_template.ws63
cargo build -Zbuild-std=core,alloc -p <name> --release
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/<name>
# docs:end

# docs:start contrib_example_template.bs21
cargo build -Zbuild-std=core,alloc --manifest-path examples/bs21/Cargo.toml --release
qemu-system-riscv32 -M bs21 -nographic -bios none \
    -kernel examples/bs21/target/riscv32imfc-unknown-none-elf/release/bs21_<name>
# docs:end

# docs:start contrib_example_template.bs20
cargo build -Zbuild-std=core,alloc --manifest-path examples/bs20/Cargo.toml --release
qemu-system-riscv32 -M bs20 -nographic -bios none \
    -kernel examples/bs20/target/riscv32imfc-unknown-none-elf/release/bs20_<name>
# docs:end

# docs:start contrib_run_blinky
cargo build -Zbuild-std=core,alloc -p blinky --release
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/blinky
# docs:end

# docs:start contrib_run_uart_hello
cargo build -Zbuild-std=core,alloc -p uart_hello --release
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/uart_hello
# docs:end

# docs:start contrib_run_timer_irq
cargo build -Zbuild-std=core,alloc -p timer_irq --release
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/timer_irq
# docs:end

# docs:start contrib_run_gpio_irq
cargo build -Zbuild-std=core,alloc -p gpio_irq --release
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/gpio_irq
# docs:end

# docs:start contrib_run_semihost_selftest
cargo build -Zbuild-std=core,alloc -p semihost_selftest --release
qemu-system-riscv32 -M ws63 -nographic -bios none -semihosting \
    -kernel target/riscv32imfc-unknown-none-elf/release/semihost_selftest
# docs:end

# docs:start contrib_semihost_exit_code
echo $?
# docs:end

# docs:start hil_flash_blinky
cargo build -Zbuild-std=core,alloc -p blinky --release

hisi-fwpkg plan target/riscv32imfc-unknown-none-elf/release/blinky \
    --chip ws63 --image-output blinky.img > blinky.plan.json

BASE_ADDR=$(uv run scripts/read-flash-plan-base.py blinky.plan.json)
probe-rs download --chip WS63 \
    --chip-description-path HiSilicon_WS63.yaml \
    --binary-format bin --base-address "$BASE_ADDR" blinky.img

probe-rs reset
# docs:end

# docs:start hil_uart_monitor
stty -F /dev/ttyUSB0 115200 raw -echo
cat /dev/ttyUSB0
# docs:end

# docs:start hil_smoke
PORT=/dev/ttyUSB0 hil/hil-smoke.sh
# docs:end
DOCS_SNIPPETS

echo "tutorial-contracts: checking hisi-riscv target"
rustc --print target-list | grep -qx 'riscv32imfc-unknown-none-elf'

echo "tutorial-contracts: checking submodules"
git -C "$ROOT" submodule status --recursive >/dev/null

echo "tutorial-contracts: building tutorial examples"
(
    cd "$ROOT"
    cargo build -Zbuild-std=core,alloc -p blinky --release
    cargo build -Zbuild-std=core,alloc -p uart_hello --release
    cargo build -Zbuild-std=core,alloc -p timer_irq --release
    cargo build -Zbuild-std=core,alloc -p gpio_irq --release
    cargo build -Zbuild-std=core,alloc -p semihost_selftest --release
)

ELF="$ROOT/target/riscv32imfc-unknown-none-elf/release/uart_hello"
IMG="$WORK/uart_hello.img"
PLAN="$WORK/uart_hello.plan.json"

echo "tutorial-contracts: planning tutorial image"
"$HISI_FWPKG" plan "$ELF" --chip ws63 --image-output "$IMG" > "$PLAN"
uv run "$ROOT/scripts/check-flash-plan.py" \
    --base-address 0x230000 "$PLAN" "$IMG"

run_template_case() {
    local chip="$1"
    local starter="$2"
    local project="$3"
    local crate="$4"
    local app_addr="$5"
    local build_image="$6"

    echo "tutorial-contracts: generating template ${chip}/${starter}"
    (
        cd "$WORK"
        cargo generate --path "$ROOT/crates/hisi-rs-template" \
            --name "$project" \
            --define "chip=$chip" \
            --define "starter=$starter" \
            --define "app_partition_addr=$app_addr" \
            --vcs none \
            --no-workspace \
            --silent
        cd "$project"
        if [ "$starter" = "wifi" ]; then
            export WS63_WIFI_SSID=ci-network
            export WS63_WIFI_PASSPHRASE=ci-passphrase
            if grep -Eq '^(hisi-rf-ws63|hisi-rf-rtos-driver|ws63-radio-sys|ws63-rf-rs)[[:space:]]*=' Cargo.toml; then
                echo "tutorial-contracts: wifi starter leaked an internal RF dependency" >&2
                exit 1
            fi
        fi
        cargo check -Zbuild-std=core,alloc
        cargo build -Zbuild-std=core,alloc --release
        just --list >/dev/null
        if [ "$build_image" = "image" ]; then
            just image
            test -s "${crate}.img"
            test -s "${crate}.plan.json"
        fi
    )
}

for chip in "${CHIPS[@]}"; do
    case "$chip" in
        ws63)
            run_template_case ws63 blinky hp-ws63-blinky hp_ws63_blinky 0x00230000 image
            run_template_case ws63 uart_hello hp-ws63-uart-hello hp_ws63_uart_hello 0x00230000 image
            run_template_case ws63 wifi hp-ws63-wifi hp_ws63_wifi 0x00230000 image
            ;;
        bs21)
            run_template_case bs21 blinky hp-bs21-blinky hp_bs21_blinky 0x00090000 noimage
            ;;
        bs20)
            run_template_case bs20 blinky hp-bs20-blinky hp_bs20_blinky 0x00090000 noimage
            ;;
        *)
            echo "tutorial-contracts: unknown chip '$chip'" >&2
            exit 2
            ;;
    esac
done

if command -v qemu-system-riscv32 >/dev/null 2>&1; then
    echo "tutorial-contracts: qemu-system-riscv32 found; tutorial QEMU marker checks remain optional"
else
    echo "tutorial-contracts: qemu-system-riscv32 not found; skipped optional QEMU marker checks"
fi

echo "tutorial-contracts: no-hardware tutorial contracts passed"
