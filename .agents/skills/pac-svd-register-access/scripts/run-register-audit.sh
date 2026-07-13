#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

echo "==> HAL register-access policy"
python3 "$ROOT/crates/hisi-hal/scripts/check-register-access.py"

if command -v uv >/dev/null 2>&1; then
    for svd_dir in \
        "$ROOT/crates/chips/ws63/ws63-pac/ws63-svd" \
        "$ROOT/crates/chips/bs2x/bs2x-pac/bs2x-svd"
    do
        if [ -f "$svd_dir/validate.py" ]; then
            echo "==> SVD schema: ${svd_dir#$ROOT/}"
            (cd "$svd_dir" && uv run validate.py)
        fi
    done
else
    echo "==> uv not found; skipping SVD schema validation"
fi

if [ "${RUN_CARGO:-0}" = "1" ]; then
    echo "==> HAL cargo check/clippy matrix"
    cargo check -Zbuild-std=core,alloc -p hisi-hal --no-default-features \
        --features chip-ws63,rt,unstable --target riscv32imfc-unknown-none-elf
    cargo clippy -Zbuild-std=core,alloc -p hisi-hal --no-default-features \
        --features chip-ws63,rt,unstable --target riscv32imfc-unknown-none-elf -- -D warnings
    cargo check -Zbuild-std=core,alloc -p hisi-hal --no-default-features \
        --features chip-bs21,rt,unstable --target riscv32imfc-unknown-none-elf
    cargo clippy -Zbuild-std=core,alloc -p hisi-hal --no-default-features \
        --features chip-bs21,rt,unstable --target riscv32imfc-unknown-none-elf -- -D warnings
else
    echo "==> RUN_CARGO=1 not set; skipping cargo matrix"
fi
