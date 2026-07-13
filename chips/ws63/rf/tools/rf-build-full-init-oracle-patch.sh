#!/usr/bin/env bash
#
# Build `wifi_init_smoke --features full-init` without using the vendor linker
# as the final application linker.
#
# Flow:
#   1. Use the HiSilicon linker once to produce an oracle ELF + linker map.
#   2. Patch vendor RF archives: resolve R_RISCV_48_LLUI from the oracle and
#      neutralize vendor-only relocation markers that upstream rust-lld rejects.
#   3. Rebuild the app with the normal Rust linker, pointing WS63_RF_LIB_DIR at
#      the patched archives.
#
# This is an RF bring-up tool, not a default user path.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
RADIO_SYS_DIR="$ROOT/crates/chips/ws63/ws63-radio-sys"
RF_DIR="$RADIO_SYS_DIR/ws63-RF"
HOST_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
RF_LINK_TARGET="${WS63_RF_LINK_TARGET:-${TMPDIR:-/tmp}/hisi-rf-link-target}"
cargo build --manifest-path "$RADIO_SYS_DIR/Cargo.toml" \
  -p hisi-rf-link --target "$HOST_TRIPLE" --target-dir "$RF_LINK_TARGET"
RF_LINK="$RF_LINK_TARGET/$HOST_TRIPLE/debug/hisi-rf-link"

SDK="${FBB_WS63_SDK:-$HOME/Documents/hispark/fbb_ws63/src}"
VENDOR_BIN="$SDK/tools/bin/compiler/riscv/cc_riscv32_musl_105/cc_riscv32_musl_fp/bin"
VENDOR_LD="${WS63_RF_VENDOR_LD:-$VENDOR_BIN/riscv32-linux-musl-ld}"

if [ ! -x "$VENDOR_LD" ]; then
  echo "ERROR: vendor linker not found: $VENDOR_LD" >&2
  echo "Set FBB_WS63_SDK=/path/to/fbb_ws63/src" >&2
  exit 2
fi

OUT_DIR="${WS63_RF_PATCH_OUT:-${TMPDIR:-/tmp}/ws63-rf-patched-libs}"
MAP="${WS63_RF_ORACLE_MAP:-${TMPDIR:-/tmp}/wifi_init_smoke-rf-oracle.map}"
FINAL_MAP="${WS63_RF_FINAL_MAP:-${TMPDIR:-/tmp}/wifi_init_smoke-rf-final.map}"
MANIFEST="${WS63_RF_PATCH_MANIFEST:-${TMPDIR:-/tmp}/wifi_init_smoke-rf-patched-relocs.jsonl}"
FEATURES="${WS63_RF_FEATURES:-full-init}"

LIBS=()
while IFS= read -r archive; do
  LIBS+=("$archive")
done < <("$RF_LINK" archive-paths wifi "$RF_DIR")

cd "$ROOT"

rm -f "$MAP" "$FINAL_MAP" "$MANIFEST"
rm -f target/riscv32imfc-unknown-none-elf/release/wifi_init_smoke
rm -f target/riscv32imfc-unknown-none-elf/release/deps/wifi_init_smoke-*

echo "== vendor oracle link =="
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_LINKER="$VENDOR_LD" \
CARGO_ENCODED_RUSTFLAGS="-Clink-arg=-Map=$MAP" \
cargo build -Zbuild-std=core,alloc -p wifi_init_smoke --release --features "$FEATURES"

ORACLE_ELF="$ROOT/target/riscv32imfc-unknown-none-elf/release/wifi_init_smoke"

rm -rf "$OUT_DIR"
echo
echo "== patch RF archives =="
"$RF_LINK" patch-from-oracle \
  --allow-missing-map \
  --map "$MAP" \
  --oracle-elf "$ORACLE_ELF" \
  --out-dir "$OUT_DIR" \
  --manifest "$MANIFEST" \
  "${LIBS[@]}"

rm -f target/riscv32imfc-unknown-none-elf/release/wifi_init_smoke
rm -f target/riscv32imfc-unknown-none-elf/release/deps/wifi_init_smoke-*

echo
echo "== stock rust-lld final link =="
WS63_RF_LIB_DIR="$OUT_DIR" \
CARGO_ENCODED_RUSTFLAGS="-Clink-arg=-Map=$FINAL_MAP" \
cargo build -Zbuild-std=core,alloc -p wifi_init_smoke --release --features "$FEATURES"

echo
echo "== verify oracle/final layout =="
if ! "$RF_LINK" verify-layout \
  --manifest "$MANIFEST" \
  --final-map "$FINAL_MAP"; then
  echo "ERROR: final rust-lld ELF layout does not match the oracle; refusing burnable output" >&2
  rm -f target/riscv32imfc-unknown-none-elf/release/wifi_init_smoke
  rm -f target/riscv32imfc-unknown-none-elf/release/deps/wifi_init_smoke-*
  exit 1
fi

echo
echo "patched RF libs: $OUT_DIR"
echo "oracle map: $MAP"
echo "final map: $FINAL_MAP"
echo "patch manifest: $MANIFEST"
echo "firmware ELF: $ROOT/target/riscv32imfc-unknown-none-elf/release/wifi_init_smoke"
