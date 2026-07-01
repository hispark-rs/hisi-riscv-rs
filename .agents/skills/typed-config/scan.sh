#!/usr/bin/env bash
# typed-config candidate scanner — heuristically flags "compiles-but-won't-run"
# config smells in a hisi-riscv-hal driver (or the whole src/ dir). Each hit is a
# CANDIDATE to investigate, not a confirmed defect: trace it to the register field
# (PAC) + valid range / clock precondition (vendor SDK), classify A/B/C/D, then
# apply the decision tree in SKILL.md.
#
# Usage:
#   bash .claude/skills/typed-config/scan.sh [path ...]
#   bash .claude/skills/typed-config/scan.sh crates/hisi-riscv-hal/src/spi.rs
#   bash .claude/skills/typed-config/scan.sh            # default: the HAL src/ dir
set -euo pipefail

targets=("$@")
if [ ${#targets[@]} -eq 0 ]; then
    # Default to the HAL src dir relative to repo root (works from repo root).
    targets=(crates/hisi-riscv-hal/src)
fi

# rg if available (faster, line numbers); else grep -rnE.
if command -v rg >/dev/null 2>&1; then
    search() { rg -n --no-heading -e "$1" "${@:2}"; }
else
    search() { grep -rnE "$1" "${@:2}"; }
fi

section() { printf '\n=== %s ===\n' "$1"; }

echo "typed-config scan: ${targets[*]}"

section "D — silent clamp/wrap (reject, don't clamp)"
search '\.clamp\(|\.min\(|\.max\(|saturating_(sub|add|mul)|== *0 *\{ *1 *\}|if .*== *0 *\{ *return|unwrap_or\(' "${targets[@]}" || echo "  (none)"

section "A — narrowing / field-mask of a computed config value (possible truncation)"
search ' as u8| as u16|& *0x[0-9A-Fa-f]{1,4}\b|>> *(8|16)' "${targets[@]}" || echo "  (none)"

section "Un-newtyped frequency/baud/period/timeout fields (candidates for a validated newtype)"
search 'pub (frequency|freq|baudrate|baud|period|timeout[a-z_]*|load[a-z_]*|div[a-z_]*) *: *u(8|16|32)' "${targets[@]}" || echo "  (none)"
search 'fn (new|new_[a-z0-9]+|configure)\([^)]*\b(freq|baud|period|timeout|load|hz)[a-z_]*: *u(8|16|32)' "${targets[@]}" || echo "  (none)"

section "B — role/mode field that may gate other fields (candidate for type-state)"
search '\b(role|mode|master|slave|Master|Slave)\b *:' "${targets[@]}" || echo "  (none)"

section "C — unbounded poll (can stall the bus) — must be bounded"
search 'while .*\{ *\}|while !.*\(\) *\{ *\}|loop *\{' "${targets[@]}" || echo "  (none)"

section "C — does the driver self-enable its clock gate? (absence = candidate)"
search 'cken|CKEN|clock_enable|enable_clock|DIV_CTL|LOAD_DIV|clk_en' "${targets[@]}" || echo "  (no clock-enable refs found — if this driver is clock-gated, configure() should self-enable it)"

cat <<'EOF'

Next: for each hit, see .claude/skills/typed-config/SKILL.md —
  trace -> classify (A/B/C/D) -> decision tree -> implement (config layer only) ->
  host tests + tests/hil.rs -> validate on the board.
Reference impl: crates/hisi-riscv-hal/src/pwm.rs (PwmPeriod / Duty).
EOF
