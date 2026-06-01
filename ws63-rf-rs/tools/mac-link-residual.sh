#!/usr/bin/env bash
#
# mac-link-residual.sh — perform the full WS63 Wi-Fi MAC link and print the
# residual (the symbols referenced by the vendor blobs but provided by nothing).
#
# It links, with rust-lld, the whole MAC blob set from the ws63-RF delivery
# against this crate (ws63-rf-rs), the WS63 mask-ROM symbol table
# (ws63-RF/rom/ws63_acore_rom.lds) and compiler-rt, two ways:
#
#   (1) full-stack   : `-r --whole-archive` over EVERY blob object — proves the
#                      whole stack links with no duplicate symbols, and prints
#                      the upper-bound residual (every object included, incl.
#                      off-path BT-coex / alternate-OS-adapter code).
#   (2) reachability : `-r --gc-sections -u uapi_wifi_init` over the blobs as
#                      normal archives — pulls only the Wi-Fi-init closure and
#                      prints the residual that actually matters for bring-up.
#
# A relocatable (`-r`) link is used deliberately: the HiSilicon-toolchain blobs
# carry custom relocations a stock lld cannot resolve to absolute addresses, and
# the ROM symbols are real-silicon addresses — so a *runnable* image is HIL, but
# the relocatable link defers relocations and gives an exact symbol residual.
#
# Pure tooling: no hardware, no C compiler. Requires the ws63 Rust toolchain
# (for rust-lld + the rv32imfc compiler_builtins) and a built ws63-rf-rs rlib.
set -u

here="$(cd "$(dirname "$0")/.." && pwd)"          # ws63-rf-rs/
root="$(cd "$here/.." && pwd)"                     # repo root
rf="$root/ws63-RF"
sysroot="$(rustc +ws63 --print sysroot 2>/dev/null || rustc --print sysroot)"
host_sysroot="$(rustc --print sysroot)"

LLD="$(find "$sysroot" "$host_sysroot" -name rust-lld 2>/dev/null | head -1)"
NM="$(find "$host_sysroot" -name llvm-nm 2>/dev/null | head -1)"
RFRS="$(find "$root/target" -name libws63_rf_rs.rlib -path '*release*' 2>/dev/null | head -1)"
BUILTINS="$(find "$sysroot" -name 'libcompiler_builtins-*.rlib' -path '*riscv32imfc*' 2>/dev/null | head -1)"
ROM="$rf/rom/ws63_acore_rom.lds"

for v in LLD NM RFRS BUILTINS; do
  if [ -z "${!v}" ] || [ ! -e "${!v}" ]; then
    echo "ERROR: $v not found (build the rlib: cargo build -p ws63-rf-rs --release)"; exit 2
  fi
done

T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
BLOBS=(
  "$rf/lib/libwifi_driver_hmac.a" "$rf/lib/libwifi_driver_dmac.a"
  "$rf/lib/libwifi_driver_tcm.a"  "$rf/lib/libbg_common.a"
  "$rf/lib/libwifi_alg_anti_interference.a" "$rf/lib/libwifi_alg_cca_opt.a"
  "$rf/lib/libwifi_alg_edca_opt.a" "$rf/lib/libwifi_alg_temp_protect.a"
  "$rf/lib/libwifi_alg_txbf.a"    "$rf/lib/libwifi_rom_data.a"
)

# ROM symbol names (name = addr;) and a filter for non-C-contract leftovers
# (Rust-internal lang items that resolve when linked into a real ws63-rt binary).
grep -oE '^[[:space:]]*[A-Za-z_][A-Za-z0-9_]*[[:space:]]*=' "$ROM" | tr -d ' =' | sort -u > "$T/rom.txt"
undef() { "$NM" "$1" 2>/dev/null | awk 'NF>=2 && $(NF-1)=="U"{print $NF}' | sort -u; }
strip_internal() { grep -vE '^_RNv|^_critical_section_|^_ZN21linked_list_allocator|^rust_'; }

echo "== (1) full-stack link (--whole-archive: every blob object) =="
"$LLD" -flavor gnu -r --whole-archive "${BLOBS[@]}" --no-whole-archive "$RFRS" "$BUILTINS" \
       -o "$T/full.o" 2>"$T/full.err"
echo "   link exit=$?  duplicate-symbols=$(grep -c 'duplicate symbol' "$T/full.err")"
undef "$T/full.o" | comm -23 - "$T/rom.txt" | strip_internal > "$T/full_resid.txt"
echo "   residual (after ROM table, C-contract): $(wc -l < "$T/full_resid.txt")"

echo "== (2) reachability link (-u uapi_wifi_init --gc-sections) =="
"$LLD" -flavor gnu -r --gc-sections -u uapi_wifi_init "${BLOBS[@]}" "$RFRS" "$BUILTINS" \
       -o "$T/reach.o" 2>"$T/reach.err"
echo "   link exit=$?"
undef "$T/reach.o" | comm -23 - "$T/rom.txt" | strip_internal > "$T/reach_resid.txt"
echo "   residual (after ROM table, C-contract): $(wc -l < "$T/reach_resid.txt")"
echo "   --- Wi-Fi-init residual ---"
sed 's/^/     /' "$T/reach_resid.txt"
echo
echo "NOTE: residual entries prefixed __wifi_pkt_ram_* are linker --defsym symbols"
echo "      (region bounds), supplied by the firmware link, not the porting layer."
