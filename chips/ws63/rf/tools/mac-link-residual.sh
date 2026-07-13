#!/usr/bin/env bash
#
# mac-link-residual.sh — perform the full WS63 Wi-Fi MAC link and print the
# residual (the symbols referenced by the vendor blobs but provided by nothing).
#
# It links, with rust-lld, the whole MAC blob set from the ws63-RF delivery
# against this crate (ws63-rf-rs), the WS63 mask-ROM symbol table
# (`ws63-radio-sys/ws63-RF/rom/ws63_acore_rom.lds`) and compiler-rt, two ways:
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
# Pure tooling: no hardware, no C compiler. Requires the official pinned Rust
# nightly used by the repo (for rust-lld + rv32imfc compiler_builtins) and a
# built ws63-rf-rs rlib. This proves the relocatable symbol closure only; use
# rf-build-full-init-lld-layout-patch.sh for the guarded final-image lane that
# resolves R_RISCV_48_LLUI while keeping rust-lld's layout authoritative.
set -u

here="$(cd "$(dirname "$0")/.." && pwd)"          # ws63-rf-rs/
root="$(cd "$here/../../.." && pwd)"               # repo root
rf="$root/crates/chips/ws63/ws63-radio-sys/ws63-RF" # owned by ws63-radio-sys
radio_sys="$root/crates/chips/ws63/ws63-radio-sys"
host="$(rustc -vV | sed -n 's/^host: //p')"
rf_link_target="${WS63_RF_LINK_TARGET:-${TMPDIR:-/tmp}/hisi-rf-link-target}"
cargo build --manifest-path "$radio_sys/Cargo.toml" \
  -p hisi-rf-link --target "$host" --target-dir "$rf_link_target"
rf_link="$rf_link_target/$host/debug/hisi-rf-link"
if [ -n "${RUSTUP_TOOLCHAIN:-}" ]; then
  sysroot="$(rustc +"$RUSTUP_TOOLCHAIN" --print sysroot 2>/dev/null || rustc --print sysroot)"
else
  sysroot="$(rustc --print sysroot)"
fi
host_sysroot="$(rustc --print sysroot)"

LLD="$(find "$sysroot" "$host_sysroot" -name rust-lld 2>/dev/null | head -1)"
NM="$(find "$host_sysroot" -name llvm-nm 2>/dev/null | head -1)"
RFRS="$(find "$root/target" -name libws63_rf_rs.rlib -path '*release*' 2>/dev/null | head -1)"
BUILTINS="$(find "$root/target" "$sysroot" -name 'libcompiler_builtins-*.rlib' -path '*riscv32imfc*' 2>/dev/null | head -1)"
ROM="$rf/rom/ws63_acore_rom.lds"

for v in LLD NM RFRS BUILTINS; do
  if [ -z "${!v}" ] || [ ! -e "${!v}" ]; then
    echo "ERROR: $v not found (build the rlib: cargo build -p ws63-rf-rs --release)"; exit 2
  fi
done

T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
BLOBS=()
while IFS= read -r archive; do BLOBS+=("$archive"); done \
  < <("$rf_link" archive-paths wifi "$rf")

# ROM symbol names (name = addr;) and a filter for non-C-contract leftovers
# (Rust-internal lang items that resolve when linked into a real hisi-riscv-rt binary).
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
echo "NOTE: residual entries prefixed __wifi_pkt_ram_* are firmware linker symbols"
echo "      (region bounds), supplied by hisi-riscv-rt's WS63 .wifi_pkt_ram section"
echo "      or by an equivalent downstream runtime memory layout."
