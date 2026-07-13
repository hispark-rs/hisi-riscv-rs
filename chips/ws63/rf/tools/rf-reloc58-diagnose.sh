#!/usr/bin/env bash
#
# Diagnose the WS63 vendor RF relocation that blocks `wifi_init_smoke
# --features full-init` with stock rust-lld.
#
# The same numeric relocation value (58) is interpreted differently by the two
# toolchains:
# - upstream LLVM/rust-lld: R_RISCV_IRELATIVE, unsupported in this final link
# - HiSilicon GNU binutils fork: R_RISCV_48_LLUI, a custom 48-bit address-load
#   relocation used by the vendor Wi-Fi objects
#
# This script is read-only and requires no board. On macOS it can still show the
# LLVM view. On Linux, set FBB_WS63_SDK=/path/to/fbb_ws63/src to also prove the
# vendor toolchain name and final-link behavior.
set -eu

here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/../../.." && pwd)"
rf="$repo/crates/ws63-radio-sys/ws63-RF"
radio_sys="$repo/crates/ws63-radio-sys"
host="$(rustc -vV | sed -n 's/^host: //p')"
rf_link_target="${WS63_RF_LINK_TARGET:-${TMPDIR:-/tmp}/hisi-rf-link-target}"
cargo build --manifest-path "$radio_sys/Cargo.toml" \
  -p hisi-rf-link --target "$host" --target-dir "$rf_link_target"
rf_link="$rf_link_target/$host/debug/hisi-rf-link"
blobs=()
while IFS= read -r archive; do blobs+=("$archive"); done \
  < <("$rf_link" archive-paths wifi "$rf")
obj_dir="${TMPDIR:-/tmp}/ws63-rf-reloc58"

sysroot="$(rustc --print sysroot)"
llvm_ar="$sysroot/lib/rustlib/$(rustc -vV | awk '/host:/ {print $2}')/bin/llvm-ar"
llvm_readobj="$sysroot/lib/rustlib/$(rustc -vV | awk '/host:/ {print $2}')/bin/llvm-readobj"

mkdir -p "$obj_dir"
rm -f "$obj_dir"/cali_iq_tone_alg.c.obj "$obj_dir"/hmac_device.c.obj
"$llvm_ar" x "$rf/lib/libwifi_driver_dmac.a" cali_iq_tone_alg.c.obj --output "$obj_dir" 2>/dev/null || {
  (cd "$obj_dir" && "$llvm_ar" x "$rf/lib/libwifi_driver_dmac.a" cali_iq_tone_alg.c.obj)
}
"$llvm_ar" x "$rf/lib/libwifi_driver_hmac.a" hmac_device.c.obj --output "$obj_dir" 2>/dev/null || {
  (cd "$obj_dir" && "$llvm_ar" x "$rf/lib/libwifi_driver_hmac.a" hmac_device.c.obj)
}

echo "== LLVM view =="
"$llvm_readobj" --relocations "$obj_dir/cali_iq_tone_alg.c.obj" |
  grep -m 12 -E 'R_RISCV_IRELATIVE|Relocations|cali_trx_iq_get_a_matrix_value|cali_trx_iq_alg' || true

sdk="${FBB_WS63_SDK:-}"
if [ -z "$sdk" ] && [ -d /home/sanchuanhehe/Documents/hispark/fbb_ws63/src ]; then
  sdk=/home/sanchuanhehe/Documents/hispark/fbb_ws63/src
fi
if [ -z "$sdk" ] || [ ! -x "$sdk/tools/bin/compiler/riscv/cc_riscv32_musl_105/cc_riscv32_musl_fp/bin/riscv32-linux-musl-ld" ]; then
  echo
  echo "Vendor toolchain not available on this host."
  echo "On Linux, rerun with FBB_WS63_SDK=/path/to/fbb_ws63/src to verify R_RISCV_48_LLUI."
  exit 0
fi

vendor_bin="$sdk/tools/bin/compiler/riscv/cc_riscv32_musl_105/cc_riscv32_musl_fp/bin"
vendor_ld="$vendor_bin/riscv32-linux-musl-ld"
vendor_readelf="$vendor_bin/riscv32-linux-musl-readelf"

echo
echo "== HiSilicon binutils view =="
"$vendor_readelf" -r "$obj_dir/cali_iq_tone_alg.c.obj" |
  grep -m 12 -E 'R_RISCV_48_LLUI|Relocation section' || true

out="$obj_dir/vendor-final.elf"
"$vendor_ld" --unresolved-symbols=ignore-all --no-relax -Ttext=0x230300 \
  -T "$rf/rom/ws63_acore_rom.lds" \
  --defsym=__wifi_pkt_ram_begin__=0x00A00000 \
  --defsym=__wifi_pkt_ram_end__=0x00A0C000 \
  -u uapi_wifi_init \
  "${blobs[@]}" \
  -o "$out"
echo
echo "Vendor final-link OK: $out"
"$vendor_readelf" -h "$out" | sed -n '8,16p'
