#!/usr/bin/env bash
set -u

ROOT="$(git rev-parse --show-toplevel)"
HAL_DIR="$ROOT/crates/hisi-riscv-hal"
OUT="$ROOT/docs/review/unsafe-audit-$(date +%F).md"
AUDIT_ONLY=0

case "${1:-}" in
  --audit-only)
    AUDIT_ONLY=1
    ;;
  "" )
    ;;
  * )
    echo "usage: $0 [--audit-only]" >&2
    exit 2
    ;;
esac

mkdir -p "$(dirname "$OUT")"
TMP_CLIPPY="$(mktemp)"
trap 'rm -f "$TMP_CLIPPY"' EXIT

unsafe_count="$(grep -RIn --include='*.rs' 'unsafe' "$HAL_DIR/src" | wc -l | tr -d ' ')"

{
  echo "# Unsafe Verification Readiness"
  echo
  echo "- Date: $(date +%F)"
  echo "- Scope: crates/hisi-riscv-hal/src"
  echo "- Unsafe occurrences: $unsafe_count"
  echo
  echo "## Unsafe Occurrences By File"
  echo
  grep -RIn --include='*.rs' 'unsafe' "$HAL_DIR/src" \
    | cut -d: -f1 \
    | sed "s#^$ROOT/##" \
    | sort \
    | uniq -c \
    | sort -nr \
    | awk '{ printf "- %s: %s\n", $2, $1 }'
  echo
  echo "## Safe To Unsafe Forwarding Candidates"
  echo
  echo "Heuristic only. Review manually; this intentionally does not decide soundness."
  echo
  grep -RIn --include='*.rs' 'pub fn' "$HAL_DIR/src" \
    | grep -v '#\[' \
    | grep -v 'test' \
    | while IFS=: read -r file line rest; do
        fn_name="$(printf '%s\n' "$rest" | sed 's/.*pub fn \([A-Za-z0-9_]*\).*/\1/')"
        if [ -n "$fn_name" ] && grep -A40 "^[[:space:]]*pub fn $fn_name" "$file" | grep -q 'unsafe'; then
          printf -- "- %s:%s: %s wraps or reaches unsafe within the next 40 lines\n" "${file#$ROOT/}" "$line" "$fn_name"
        fi
      done
} > "$OUT"

if [ "$AUDIT_ONLY" -eq 0 ]; then
  {
    echo
    echo "## Clippy undocumented_unsafe_blocks Baseline"
    echo
    echo '```text'
  } >> "$OUT"

  (
    cd "$ROOT"
    cargo clippy -p hisi-riscv-hal --no-deps --no-default-features --features chip-ws63 \
      --target x86_64-unknown-linux-gnu -- \
      -W clippy::undocumented_unsafe_blocks
  ) >"$TMP_CLIPPY" 2>&1
  clippy_status=$?

  undocumented_count="$(grep -c 'unsafe .*missing a safety comment' "$TMP_CLIPPY" || true)"
  tail -80 "$TMP_CLIPPY" >> "$OUT"
  {
    echo '```'
    echo
    echo "- Clippy exit status: $clippy_status"
    echo "- Undocumented unsafe warnings in captured output: $undocumented_count"
    echo
    echo "## Miri Readiness"
    echo
  } >> "$OUT"

  if command -v cargo >/dev/null 2>&1 && rustup +nightly component list --installed 2>/dev/null | grep -q '^miri '; then
    echo "- Miri is installed for nightly. Run host-testable paths manually." >> "$OUT"
  else
    echo "- Miri is not installed or nightly is unavailable. No Miri gate ran." >> "$OUT"
  fi

  {
    echo
    echo "## Kani Readiness"
    echo
  } >> "$OUT"
  if command -v cargo-kani >/dev/null 2>&1 || command -v kani >/dev/null 2>&1; then
    echo "- Kani tooling appears available, but this repo still needs explicit harnesses." >> "$OUT"
  else
    echo "- Kani tooling not found. No model-checking gate ran." >> "$OUT"
  fi
fi

echo "wrote $OUT"
