#!/usr/bin/env bash
#
# Build `wifi_init_smoke --features full-init` with stock rust-lld by resolving
# HiSilicon R_RISCV_48_LLUI relocations from rust-lld's own final layout.
#
# Flow:
#   1. Neutralize vendor-only relocation types so rust-lld can produce a layout
#      ELF + map. Instruction immediates are still untrusted in this pass.
#   2. Re-patch the original RF archives using that rust-lld map/symtab.
#   3. Re-link with stock rust-lld and emit a burnable ELF only if all patch
#      sites resolved from the rust-lld layout.
#
# This is an RF bring-up tool, not a default user path.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
RF_DIR="$ROOT/chips/ws63/rf/ws63-RF"
ROM_SYMBOLS="$RF_DIR/rom/ws63_acore_rom.lds"
ROM_PATCHES="$RF_DIR/rom/ws63_acore_wifi_patches.txt"
LLVM_NM="$(find "$(rustc --print sysroot)" -name llvm-nm -type f | head -1)"
LLVM_OBJCOPY="$(find "$(rustc --print sysroot)" -name llvm-objcopy -type f | head -1)"
LLVM_AR="$(find "$(rustc --print sysroot)" -name llvm-ar -type f | head -1)"
HISI_FWPKG="${HISI_FWPKG:-hisi-fwpkg}"

command -v "$HISI_FWPKG" >/dev/null 2>&1 || {
  echo "ERROR: '$HISI_FWPKG' is required to seal the post-link ELF hash" >&2
  exit 1
}

NEUTRAL_DIR="${WS63_RF_NEUTRAL_OUT:-${TMPDIR:-/tmp}/ws63-rf-neutral-libs}"
PATCHED_DIR="${WS63_RF_PATCH_OUT:-${TMPDIR:-/tmp}/ws63-rf-lld-layout-patched-libs}"
DIAG_SOURCE_DIR="${WS63_RF_DIAG_SOURCE_OUT:-${TMPDIR:-/tmp}/ws63-rf-diag-source-libs}"
LAYOUT_MAP="${WS63_RF_LAYOUT_MAP:-${TMPDIR:-/tmp}/wifi_init_smoke-rf-lld-layout.map}"
FINAL_MAP="${WS63_RF_FINAL_MAP:-${TMPDIR:-/tmp}/wifi_init_smoke-rf-lld-final.map}"
MANIFEST="${WS63_RF_PATCH_MANIFEST:-${TMPDIR:-/tmp}/wifi_init_smoke-rf-lld-patched-relocs.jsonl}"
FEATURES="${WS63_RF_FEATURES:-full-init}"
LAYOUT_RUSTFLAGS="-Clink-arg=--no-relax"$'\x1f'"-Clink-arg=-Map=$LAYOUT_MAP"
FINAL_RUSTFLAGS="-Clink-arg=--no-relax"$'\x1f'"-Clink-arg=-Map=$FINAL_MAP"

LIBS=(
  "$RF_DIR/lib/libwifi_driver_hmac.a"
  "$RF_DIR/lib/libwifi_driver_dmac.a"
  "$RF_DIR/lib/libwifi_driver_tcm.a"
  "$RF_DIR/lib/libbg_common.a"
  "$RF_DIR/lib/libwifi_alg_anti_interference.a"
  "$RF_DIR/lib/libwifi_alg_cca_opt.a"
  "$RF_DIR/lib/libwifi_alg_edca_opt.a"
  "$RF_DIR/lib/libwifi_alg_temp_protect.a"
  "$RF_DIR/lib/libwifi_alg_txbf.a"
  "$RF_DIR/lib/libwifi_rom_data.a"
)
WPA_ARCHIVE=""

case ",$FEATURES," in
  *,wpa,*)
    SDK_APP_OUT="${WS63_SDK_APP_OUT:-/Users/sanchuan/Documents/hispark/fbb_ws63/src/output/ws63/acore/ws63-liteos-app}"
    WPA_ARCHIVE="${WS63_WPA_ARCHIVE:-$RF_DIR/lib/libwpa_supplicant.a}"
    test -f "$WPA_ARCHIVE" || {
      echo "ERROR: WPA archive not found: $WPA_ARCHIVE" >&2
      exit 1
    }
    LIBS+=(
      "$WPA_ARCHIVE"
      "$SDK_APP_OUT/driver/security_unified/libdrv_security_unified.a"
      "$SDK_APP_OUT/hal/security_unified/libhal_security_unified.a"
      "$SDK_APP_OUT/liteos/libs/libc.a"
      "$SDK_APP_OUT/liteos/libs/libm.a"
    )
    ;;
esac

rename_rf_diag_symbol() {
  local archive="$1/libwifi_driver_dmac.a"
  local rewritten="$archive.rewritten"
  local work
  work="$(mktemp -d)"
  "$LLVM_AR" t "$archive" > "$work/members.txt"
  (
    cd "$work"
    "$LLVM_AR" x "$archive"
    test -f fe_rf_dev_attach.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym fe_rf_dev_set_ops_ext=__ws63_vendor_fe_rf_dev_set_ops_ext \
      fe_rf_dev_attach.c.obj fe_rf_dev_attach.c.obj.rewritten
    mv fe_rf_dev_attach.c.obj.rewritten fe_rf_dev_attach.c.obj
    local members=()
    while IFS= read -r member; do
      members+=("$member")
    done < members.txt
    "$LLVM_AR" rcs "$rewritten" "${members[@]}"
  )
  rm -rf "$work"
  mv "$rewritten" "$archive"

  archive="$1/libwifi_driver_hmac.a"
  rewritten="$archive.rewritten"
  work="$(mktemp -d)"
  "$LLVM_AR" t "$archive" > "$work/members.txt"
  (
    cd "$work"
    "$LLVM_AR" x "$archive"
    test -f hmac_main.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym hmac_main_init_later=__ws63_vendor_hmac_main_init_later \
      hmac_main.c.obj hmac_main.c.obj.rewritten
    mv hmac_main.c.obj.rewritten hmac_main.c.obj
    test -f wal_common.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym wal_sync_post2hmac_no_rsp=__ws63_vendor_wal_sync_post2hmac_no_rsp \
      --redefine-sym wal_sync_send2device_no_rsp=__ws63_vendor_wal_sync_send2device_no_rsp \
      wal_common.c.obj wal_common.c.obj.rewritten
    mv wal_common.c.obj.rewritten wal_common.c.obj
    local members=()
    while IFS= read -r member; do
      members+=("$member")
    done < members.txt
    "$LLVM_AR" rcs "$rewritten" "${members[@]}"
  )
  rm -rf "$work"
  mv "$rewritten" "$archive"

  archive="$1/libwifi_driver_tcm.a"
  rewritten="$archive.rewritten"
  work="$(mktemp -d)"
  "$LLVM_AR" t "$archive" > "$work/members.txt"
  (
    cd "$work"
    "$LLVM_AR" x "$archive"
    test -f oal_net.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym oal_get_netdev_by_name=__ws63_vendor_oal_get_netdev_by_name \
      --redefine-sym oal_net_register_netdev=__ws63_vendor_oal_net_register_netdev \
      oal_net.c.obj oal_net.c.obj.rewritten
    mv oal_net.c.obj.rewritten oal_net.c.obj
    test -f frw_hmac.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym frw_sync_host_post_msg=__ws63_vendor_frw_sync_host_post_msg \
      frw_hmac.c.obj frw_hmac.c.obj.rewritten
    mv frw_hmac.c.obj.rewritten frw_hmac.c.obj
    test -f frw_thread.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym frw_send_cfg_to_device_sync=__ws63_vendor_frw_send_cfg_to_device_sync \
      frw_thread.c.obj frw_thread.c.obj.rewritten
    mv frw_thread.c.obj.rewritten frw_thread.c.obj
    local members=()
    while IFS= read -r member; do
      members+=("$member")
    done < members.txt
    "$LLVM_AR" rcs "$rewritten" "${members[@]}"
  )
  rm -rf "$work"
  mv "$rewritten" "$archive"
}

rename_wpa_libc_conflicts() {
  local archive="$1/libc.a"
  local rewritten="$archive.rewritten"
  local work
  work="$(mktemp -d)"
  "$LLVM_AR" t "$archive" > "$work/members.txt"
  (
    cd "$work"
    "$LLVM_AR" x "$archive"
    test -f cstdlib.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym malloc=__ws63_vendor_libc_malloc \
      --redefine-sym free=__ws63_vendor_libc_free \
      --redefine-sym realloc=__ws63_vendor_libc_realloc \
      cstdlib.c.obj cstdlib.c.obj.rewritten
    mv cstdlib.c.obj.rewritten cstdlib.c.obj
    test -f time.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym gettimeofday=__ws63_vendor_libc_gettimeofday \
      --redefine-sym clock_gettime=__ws63_vendor_libc_clock_gettime \
      time.c.obj time.c.obj.rewritten
    mv time.c.obj.rewritten time.c.obj
    local members=()
    while IFS= read -r member; do members+=("$member"); done < members.txt
    "$LLVM_AR" rcs "$rewritten" "${members[@]}"
  )
  rm -rf "$work"
  mv "$rewritten" "$archive"
}

prepare_rf_diag_sources() {
  rm -rf "$DIAG_SOURCE_DIR"
  mkdir -p "$DIAG_SOURCE_DIR"
  local archive
  for archive in "${LIBS[@]}"; do
    local destination="$DIAG_SOURCE_DIR/$(basename "$archive")"
    case "$archive" in
      "$WPA_ARCHIVE") destination="$DIAG_SOURCE_DIR/libwpa_supplicant.a" ;;
    esac
    cp "$archive" "$destination"
  done
  case ",$FEATURES," in
    *,rf-init-diag,*) rename_rf_diag_symbol "$DIAG_SOURCE_DIR" ;;
  esac
  case ",$FEATURES," in
    *,wpa,*) rename_wpa_libc_conflicts "$DIAG_SOURCE_DIR" ;;
  esac
}

DIAG_LIBS=()
for archive in "${LIBS[@]}"; do
  case "$archive" in
    "$WPA_ARCHIVE") DIAG_LIBS+=("$DIAG_SOURCE_DIR/libwpa_supplicant.a") ;;
    *) DIAG_LIBS+=("$DIAG_SOURCE_DIR/$(basename "$archive")") ;;
  esac
done

cd "$ROOT"

rm -rf "$NEUTRAL_DIR" "$PATCHED_DIR"
rm -f "$LAYOUT_MAP" "$FINAL_MAP" "$MANIFEST"
rm -f target/riscv32imfc-unknown-none-elf/release/wifi_init_smoke
rm -f target/riscv32imfc-unknown-none-elf/release/deps/wifi_init_smoke-*

prepare_rf_diag_sources

echo "== neutralize RF archives for rust-lld layout pass =="
python3 "$SCRIPT_DIR/rf-patch-reloc58-from-layout.py" \
  --mode neutralize \
  --out-dir "$NEUTRAL_DIR" \
  "${DIAG_LIBS[@]}"

echo
echo "== stock rust-lld layout pass =="
WS63_RF_LIB_DIR="$NEUTRAL_DIR" \
CARGO_ENCODED_RUSTFLAGS="$LAYOUT_RUSTFLAGS" \
cargo build -Zbuild-std=core,alloc -p wifi_init_smoke --release --features "$FEATURES"

LAYOUT_ELF="$ROOT/target/riscv32imfc-unknown-none-elf/release/wifi_init_smoke"

echo
echo "== patch RF archives from rust-lld layout =="
python3 "$SCRIPT_DIR/rf-patch-reloc58-from-layout.py" \
  --mode patch \
  --allow-missing-layout \
  --final-map "$LAYOUT_MAP" \
  --final-elf "$LAYOUT_ELF" \
  --out-dir "$PATCHED_DIR" \
  --manifest "$MANIFEST" \
  "${DIAG_LIBS[@]}"

rm -f target/riscv32imfc-unknown-none-elf/release/wifi_init_smoke
rm -f target/riscv32imfc-unknown-none-elf/release/deps/wifi_init_smoke-*

echo
echo "== stock rust-lld final link =="
WS63_RF_LIB_DIR="$PATCHED_DIR" \
CARGO_ENCODED_RUSTFLAGS="$FINAL_RUSTFLAGS" \
cargo build -Zbuild-std=core,alloc -p wifi_init_smoke --release --features "$FEATURES"

echo
echo "== verify layout/final map =="
if ! python3 "$SCRIPT_DIR/rf-verify-oracle-layout.py" \
  --manifest "$MANIFEST" \
  --final-map "$FINAL_MAP" \
  --final-elf "$LAYOUT_ELF"; then
  echo "ERROR: final rust-lld layout changed after patching; refusing burnable output" >&2
  rm -f target/riscv32imfc-unknown-none-elf/release/wifi_init_smoke
  rm -f target/riscv32imfc-unknown-none-elf/release/deps/wifi_init_smoke-*
  exit 1
fi

echo
echo "== generate mask-ROM patch table from final ELF =="
python3 "$SCRIPT_DIR/rf-generate-rom-patch.py" \
  --elf "$LAYOUT_ELF" \
  --llvm-nm "$LLVM_NM" \
  --rom-symbols "$ROM_SYMBOLS" \
  --patch-list "$ROM_PATCHES" \
  --expected-count 37 \
  --report "${WS63_RF_ROM_PATCH_REPORT:-${TMPDIR:-/tmp}/wifi_init_smoke-rom-patches.json}"

# The ROM patch table is generated after the final link, so it changes bytes in
# the verified flash body. Seal the boot header only after every post-link
# transform; otherwise flashboot correctly rejects the image with `VE`.
"$HISI_FWPKG" patch-hash "$LAYOUT_ELF"

verify_rom_symbol() {
  local symbol="$1"
  local expected actual
  expected="$(awk -v symbol="$symbol" '$1 == symbol && $2 == "=" { gsub(/;/, "", $3); print $3; exit }' "$ROM_SYMBOLS")"
  actual="$("$LLVM_NM" -n "$LAYOUT_ELF" | awk -v symbol="$symbol" '$NF == symbol { value = "0x" $1 } END { print value }')"
  if [ -z "$expected" ] || [ -z "$actual" ] || (( actual != expected )); then
    echo "ERROR: ROM symbol $symbol resolved to ${actual:-missing}, expected ${expected:-missing}" >&2
    rm -f "$LAYOUT_ELF"
    exit 1
  fi
  echo "verified ROM symbol $symbol = $actual"
}

verify_rom_symbol frw_rom_cb_register

echo
echo "neutral RF libs: $NEUTRAL_DIR"
echo "patched RF libs: $PATCHED_DIR"
echo "layout map: $LAYOUT_MAP"
echo "final map: $FINAL_MAP"
echo "patch manifest: $MANIFEST"
echo "ROM patch report: ${WS63_RF_ROM_PATCH_REPORT:-${TMPDIR:-/tmp}/wifi_init_smoke-rom-patches.json}"
echo "firmware ELF: $ROOT/target/riscv32imfc-unknown-none-elf/release/wifi_init_smoke"
