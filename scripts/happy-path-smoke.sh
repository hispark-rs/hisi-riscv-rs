#!/usr/bin/env bash
# CI-safe happy-path smoke: build firmware, package through hisi-fwpkg plan, and
# generate/check the user template. Hardware flashing remains in HIL workflows.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NO_HARDWARE=0

for arg in "$@"; do
    case "$arg" in
        --no-hardware) NO_HARDWARE=1 ;;
        *)
            echo "usage: $0 [--no-hardware]" >&2
            exit 2
            ;;
    esac
done

if [ "$NO_HARDWARE" -ne 1 ]; then
    echo "happy-path-smoke: only --no-hardware is supported here; use HIL for board runs" >&2
    exit 2
fi

HISI_FWPKG="${HISI_FWPKG:-hisi-fwpkg}"
TMPDIR="${TMPDIR:-/tmp}"
WORK="$(mktemp -d "$TMPDIR/hisi-happy-path.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

need() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "happy-path-smoke: missing required command: $1" >&2
        exit 127
    }
}

need cargo
need cargo-generate
need just
need python3
need "$HISI_FWPKG"

echo "happy-path: checking hisi-riscv target"
rustc --print target-list | grep -qx 'riscv32imfc-unknown-none-elf'

echo "happy-path: building uart_hello example"
(
    cd "$ROOT"
    cargo build -p uart_hello --release
)

ELF="$ROOT/target/riscv32imfc-unknown-none-elf/release/uart_hello"
IMG="$WORK/uart_hello.img"
PLAN="$WORK/uart_hello.plan.json"

echo "happy-path: planning example image"
"$HISI_FWPKG" plan "$ELF" --chip ws63 --image-output "$IMG" > "$PLAN"
python3 - "$PLAN" "$IMG" <<'PY'
import json
import os
import sys

plan_path, image_path = sys.argv[1], sys.argv[2]
with open(plan_path, "r", encoding="utf-8") as f:
    plan = json.load(f)

required = [
    "base_addr",
    "image_len",
    "body_range",
    "code_area_len",
    "code_area_hash",
    "erase_range",
    "write_chunks",
]
missing = [key for key in required if key not in plan]
if missing:
    raise SystemExit(f"missing plan keys: {missing}")
if plan["base_addr"] != 0x230000:
    raise SystemExit(f"unexpected WS63 base_addr: {plan['base_addr']:#x}")
if plan["image_len"] != os.path.getsize(image_path):
    raise SystemExit("plan image_len does not match image file size")
if not plan["write_chunks"]:
    raise SystemExit("write_chunks must not be empty")
PY

run_template_case() {
    local chip="$1"
    local starter="$2"
    local project="$3"
    local crate="$4"
    local app_addr="$5"
    local build_image="$6"

    echo "happy-path: generating template ${chip}/${starter}"
    (
        cd "$WORK"
        cargo generate --path "$ROOT/crates/hisi-rs-template" \
            --name "$project" \
            --define "chip=$chip" \
            --define "starter=$starter" \
            --define "app_partition_addr=$app_addr" \
            --vcs none \
            --silent
        cd "$project"
        cargo check
        cargo build --release
        just --list >/dev/null
        if [ "$build_image" = "image" ]; then
            just image
            test -s "${crate}.img"
            test -s "${crate}.plan.json"
        fi
    )
}

run_template_case ws63 blinky hp-ws63-blinky hp_ws63_blinky 0x00230000 image
run_template_case ws63 uart_hello hp-ws63-uart-hello hp_ws63_uart_hello 0x00230000 image
run_template_case bs21 blinky hp-bs21-blinky hp_bs21_blinky 0x00090000 noimage

echo "happy-path: no-hardware smoke passed"

