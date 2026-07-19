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
RADIO_SYS_DIR="$ROOT/crates/chips/ws63/ws63-radio-sys"
RF_DIR="$RADIO_SYS_DIR/ws63-RF"
TASK_PROFILE="$RADIO_SYS_DIR/crates/hisi-rf-link/profiles/ws63-scheduling.toml"
ROM_SYMBOLS="$RF_DIR/rom/ws63_acore_rom.lds"
ROM_PATCHES="$RF_DIR/rom/ws63_acore_wifi_patches.txt"
LLVM_NM="$(find "$(rustc --print sysroot)" -name llvm-nm -type f | head -1)"
LLVM_OBJCOPY="$(find "$(rustc --print sysroot)" -name llvm-objcopy -type f | head -1)"
LLVM_AR="$(find "$(rustc --print sysroot)" -name llvm-ar -type f | head -1)"
HISI_FWPKG="${HISI_FWPKG:-hisi-fwpkg}"
HOST_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
RF_LINK_TARGET="${WS63_RF_LINK_TARGET:-${TMPDIR:-/tmp}/hisi-rf-link-target}"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
case "$TARGET_DIR" in
  /*) ;;
  *) TARGET_DIR="$ROOT/$TARGET_DIR" ;;
esac
FIRMWARE_DIR="$TARGET_DIR/riscv32imfc-unknown-none-elf/release"
LAYOUT_ELF="$FIRMWARE_DIR/wifi_init_smoke"

cargo build --manifest-path "$RADIO_SYS_DIR/Cargo.toml" \
  -p hisi-rf-link --target "$HOST_TRIPLE" --target-dir "$RF_LINK_TARGET"
RF_LINK="$RF_LINK_TARGET/$HOST_TRIPLE/debug/hisi-rf-link"

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
TASK_PROFILE_REPORT="${WS63_RF_TASK_PROFILE_REPORT:-${TMPDIR:-/tmp}/wifi_init_smoke-task-profile.json}"
FEATURES="${WS63_RF_FEATURES:-full-init}"
WPA_PROFILE=""
LAYOUT_RUSTFLAGS="-Clink-arg=--no-relax"$'\x1f'"-Clink-arg=-Map=$LAYOUT_MAP"
FINAL_RUSTFLAGS="-Clink-arg=--no-relax"$'\x1f'"-Clink-arg=-Map=$FINAL_MAP"

LIBS=()
while IFS= read -r archive; do
  LIBS+=("$archive")
done < <("$RF_LINK" archive-paths wifi "$RF_DIR")
WPA_ARCHIVE=""

case ",$FEATURES," in
  *,wpa,*wpa3,*|*,wpa3,*wpa,*)
    echo "ERROR: select exactly one of the wpa or wpa3 firmware features" >&2
    exit 1
    ;;
  *,wpa3,*) WPA_PROFILE="wpa3-personal" ;;
  *,wpa,*) WPA_PROFILE="wpa2-personal" ;;
esac

case "$WPA_PROFILE" in
  wpa2-personal|wpa3-personal)
    SDK_APP_OUT="${WS63_SDK_APP_OUT:-/Users/sanchuan/Documents/hispark/fbb_ws63/src/output/ws63/acore/ws63-liteos-app}"
    WPA_ARCHIVE="${WS63_WPA_ARCHIVE:-}"
    test -n "$WPA_ARCHIVE" || {
      echo "ERROR: $WPA_PROFILE requires its explicit profile archive" >&2
      echo "Set WS63_WPA_ARCHIVE to the WPA archive selected by the radio profile" >&2
      echo "Build it with chips/ws63/rf/tools/build-wpa2-personal.py" >&2
      exit 1
    }
    test -f "$WPA_ARCHIVE" || {
      echo "ERROR: WPA archive not found: $WPA_ARCHIVE" >&2
      exit 1
    }
    EXPECTED_WPA_SHA="$(
      uv run "$SCRIPT_DIR/read-task-profile-hash.py" "$TASK_PROFILE" "$WPA_PROFILE"
    )"
    ACTUAL_WPA_SHA="$(shasum -a 256 "$WPA_ARCHIVE" | awk '{print $1}')"
    test "$ACTUAL_WPA_SHA" = "$EXPECTED_WPA_SHA" || {
      echo "ERROR: WPA archive does not match the selected task profile" >&2
      echo "  expected: $EXPECTED_WPA_SHA" >&2
      echo "  actual:   $ACTUAL_WPA_SHA" >&2
      echo "  archive:  $WPA_ARCHIVE" >&2
      exit 1
    }
    while IFS= read -r archive; do
      LIBS+=("$archive")
    done < <("$RF_LINK" archive-paths wpa "$SDK_APP_OUT" "$WPA_ARCHIVE" "$WPA_PROFILE")
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

rename_wpa_diag_symbols() {
  local archive="$1/libwpa_supplicant.a"
  local rewritten="$archive.rewritten"
  local work
  work="$(mktemp -d)"
  "$LLVM_AR" t "$archive" > "$work/members.txt"
  (
    cd "$work"
    "$LLVM_AR" x "$archive"
    local eloop
    mapfile -t members < <(find . -maxdepth 1 -type f -name '*-eloop_rtos.o' -print)
    test "${#members[@]}" -eq 1
    eloop="${members[0]#./}"
    "$LLVM_OBJCOPY" \
      --redefine-sym eloop_post_event=__ws63_vendor_eloop_post_event \
      --redefine-sym eloop_read_event=__ws63_vendor_eloop_read_event \
      "$eloop" "$eloop.rewritten"
    mv "$eloop.rewritten" "$eloop"
    local driver_soc
    mapfile -t members < <(find . -maxdepth 1 -type f -name '*-driver_soc.o' -print)
    test "${#members[@]}" -eq 1
    driver_soc="${members[0]#./}"
    "$LLVM_OBJCOPY" \
      --globalize-symbol drv_soc_driver_event_process \
      --globalize-symbol drv_soc_driver_ap_event_process \
      "$driver_soc" "$driver_soc.rewritten"
    mv "$driver_soc.rewritten" "$driver_soc"
    local events
    mapfile -t members < <(find . -maxdepth 1 -type f -name '*-events.o' -print)
    test "${#members[@]}" -eq 1
    events="${members[0]#./}"
    "$LLVM_OBJCOPY" \
      --redefine-sym wpa_supplicant_event=__ws63_vendor_wpa_supplicant_event \
      "$events" "$events.rewritten"
    mv "$events.rewritten" "$events"
    local members=()
    while IFS= read -r member; do members+=("$member"); done < members.txt
    "$LLVM_AR" rcs "$rewritten" "${members[@]}"
  )
  rm -rf "$work"
  mv "$rewritten" "$archive"
}

rename_auth_diag_symbols() {
  local dmac_archive="$1/libwifi_driver_dmac.a"
  local dmac_rewritten="$dmac_archive.rewritten"
  local dmac_work
  dmac_work="$(mktemp -d)"
  "$LLVM_AR" t "$dmac_archive" > "$dmac_work/members.txt"
  (
    cd "$dmac_work"
    "$LLVM_AR" x "$dmac_archive"
    test -f dmac_forward_main.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym dmac_tx_complete_event_handler=__ws63_diag_dmac_tx_complete_event_handler \
      dmac_forward_main.c.obj dmac_forward_main.c.obj.rewritten
    mv dmac_forward_main.c.obj.rewritten dmac_forward_main.c.obj
    test -f dmac_wifi_patch.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym dmac_rx_prepare_data_patch=__ws63_vendor_dmac_rx_prepare_data_patch \
      dmac_wifi_patch.c.obj dmac_wifi_patch.c.obj.rewritten
    mv dmac_wifi_patch.c.obj.rewritten dmac_wifi_patch.c.obj
    local members=()
    while IFS= read -r member; do members+=("$member"); done < members.txt
    "$LLVM_AR" rcs "$dmac_rewritten" "${members[@]}"
  )
  rm -rf "$dmac_work"
  mv "$dmac_rewritten" "$dmac_archive"

  local tcm_archive="$1/libwifi_driver_tcm.a"
  local tcm_rewritten="$tcm_archive.rewritten"
  local tcm_work
  tcm_work="$(mktemp -d)"
  "$LLVM_AR" t "$tcm_archive" > "$tcm_work/members.txt"
  (
    cd "$tcm_work"
    "$LLVM_AR" x "$tcm_archive"
    test -f wal_net.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym hmac_bridge_vap_xmit_etc=__ws63_diag_hmac_bridge_vap_xmit_etc \
      wal_net.c.obj wal_net.c.obj.rewritten
    mv wal_net.c.obj.rewritten wal_net.c.obj
    test -f hmac_rx_data.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym hwal_netif_rx=__ws63_diag_hwal_netif_rx \
      hmac_rx_data.c.obj hmac_rx_data.c.obj.rewritten
    mv hmac_rx_data.c.obj.rewritten hmac_rx_data.c.obj
    local members=()
    while IFS= read -r member; do members+=("$member"); done < members.txt
    "$LLVM_AR" rcs "$tcm_rewritten" "${members[@]}"
  )
  rm -rf "$tcm_work"
  mv "$tcm_rewritten" "$tcm_archive"

  local archive="$1/libwifi_driver_hmac.a"
  local rewritten="$archive.rewritten"
  local work
  work="$(mktemp -d)"
  "$LLVM_AR" t "$archive" > "$work/members.txt"
  (
    cd "$work"
    "$LLVM_AR" x "$archive"
    test -f hmac_mgmt_sta.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym hmac_sta_wait_auth_seq2_rx_etc=__ws63_vendor_hmac_sta_wait_auth_seq2_rx_etc \
      --redefine-sym hmac_sta_auth_timeout_etc=__ws63_vendor_hmac_sta_auth_timeout_etc \
      hmac_mgmt_sta.c.obj hmac_mgmt_sta.c.obj.rewritten
    mv hmac_mgmt_sta.c.obj.rewritten hmac_mgmt_sta.c.obj
    test -f hmac_mgmt_classifier.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym hmac_rx_mgmt_event_adapt=__ws63_vendor_hmac_rx_mgmt_event_adapt \
      hmac_mgmt_classifier.c.obj hmac_mgmt_classifier.c.obj.rewritten
    mv hmac_mgmt_classifier.c.obj.rewritten hmac_mgmt_classifier.c.obj
    test -f hmac_mgmt_bss_comm.c.obj
    "$LLVM_OBJCOPY" \
      --redefine-sym hmac_tx_mgmt_send_event_etc=__ws63_vendor_hmac_tx_mgmt_send_event_etc \
      hmac_mgmt_bss_comm.c.obj hmac_mgmt_bss_comm.c.obj.rewritten
    mv hmac_mgmt_bss_comm.c.obj.rewritten hmac_mgmt_bss_comm.c.obj
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
    *,wpa,*|*,wpa3,*) rename_wpa_libc_conflicts "$DIAG_SOURCE_DIR" ;;
  esac
  case ",$FEATURES," in
    *,rf-init-diag,*|*,rf-eloop-diag,*)
      case ",$FEATURES," in
        *,wpa,*|*,wpa3,*) rename_wpa_diag_symbols "$DIAG_SOURCE_DIR" ;;
      esac
      ;;
  esac
  case ",$FEATURES," in
    *,rf-eloop-diag,*) rename_auth_diag_symbols "$DIAG_SOURCE_DIR" ;;
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
rm -f "$LAYOUT_ELF"
rm -f "$FIRMWARE_DIR"/deps/wifi_init_smoke-*

prepare_rf_diag_sources

echo "== neutralize RF archives for rust-lld layout pass =="
"$RF_LINK" patch-reloc \
  --mode neutralize \
  --out-dir "$NEUTRAL_DIR" \
  "${DIAG_LIBS[@]}"

echo
echo "== stock rust-lld layout pass =="
WS63_RF_LIB_DIR="$NEUTRAL_DIR" \
CARGO_ENCODED_RUSTFLAGS="$LAYOUT_RUSTFLAGS" \
CARGO_TARGET_DIR="$TARGET_DIR" \
cargo build -Zbuild-std=core,alloc -p wifi_init_smoke --release --features "$FEATURES"

echo
echo "== patch RF archives from rust-lld layout =="
"$RF_LINK" patch-reloc \
  --mode patch \
  --allow-missing-layout \
  --final-map "$LAYOUT_MAP" \
  --final-elf "$LAYOUT_ELF" \
  --out-dir "$PATCHED_DIR" \
  --manifest "$MANIFEST" \
  "${DIAG_LIBS[@]}"

rm -f "$LAYOUT_ELF"
rm -f "$FIRMWARE_DIR"/deps/wifi_init_smoke-*

echo
echo "== stock rust-lld final link =="
WS63_RF_LIB_DIR="$PATCHED_DIR" \
CARGO_ENCODED_RUSTFLAGS="$FINAL_RUSTFLAGS" \
CARGO_TARGET_DIR="$TARGET_DIR" \
cargo build -Zbuild-std=core,alloc -p wifi_init_smoke --release --features "$FEATURES"

echo
echo "== verify layout/final map =="
if ! "$RF_LINK" verify-layout \
  --manifest "$MANIFEST" \
  --final-map "$FINAL_MAP" \
  --final-elf "$LAYOUT_ELF"; then
  echo "ERROR: final rust-lld layout changed after patching; refusing burnable output" >&2
  rm -f "$LAYOUT_ELF"
  rm -f "$FIRMWARE_DIR"/deps/wifi_init_smoke-*
  exit 1
fi

case ",$FEATURES," in
  *,upstream-supplicant,*)
    echo
    echo "== verify upstream supplicant final-link boundary =="
    if ! uv run "$ROOT/scripts/check-ws63-supplicant-boundary.py" \
      --map "$FINAL_MAP" \
      --elf "$LAYOUT_ELF"; then
      echo "ERROR: refusing upstream image with legacy supplicant inputs" >&2
      rm -f "$LAYOUT_ELF"
      exit 1
    fi
    ;;
esac

echo
echo "== generate mask-ROM patch table from final ELF =="
"$RF_LINK" generate-rom-patch \
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
echo "== generate archive-bound task profile report =="
TASK_PROFILE_ARGS=(--elf "$LAYOUT_ELF")
if [ -n "$WPA_PROFILE" ]; then
  TASK_PROFILE_ARGS+=(--wpa-profile "$WPA_PROFILE")
fi
if [ -n "${WS63_RF_TASK_LOG:-}" ]; then
  TASK_PROFILE_ARGS+=(--log "$WS63_RF_TASK_LOG")
fi
"$RF_LINK" task-profile "${TASK_PROFILE_ARGS[@]}" > "$TASK_PROFILE_REPORT"

echo
echo "neutral RF libs: $NEUTRAL_DIR"
echo "patched RF libs: $PATCHED_DIR"
echo "layout map: $LAYOUT_MAP"
echo "final map: $FINAL_MAP"
echo "patch manifest: $MANIFEST"
echo "ROM patch report: ${WS63_RF_ROM_PATCH_REPORT:-${TMPDIR:-/tmp}/wifi_init_smoke-rom-patches.json}"
echo "task profile report: $TASK_PROFILE_REPORT"
echo "firmware ELF: $LAYOUT_ELF"
