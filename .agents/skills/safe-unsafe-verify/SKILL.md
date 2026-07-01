---
name: safe-unsafe-verify
description: Run SAFETY comment audit, unsound-encapsulation scan, and Kani verification on hisi-riscv-hal unsafe code. Use during the safe/unsafe verification cycle (P0–P3) to validate soundness before a release.
disable-model-invocation: true
---

# Safe/Unsafe Verification Skill

Runs the layered verification pipeline from the research report
(`docs/review/safe-unsafe-formal-verification-research-2026-07.md`)
on hisi-riscv-hal unsafe code:

| Step | What | Tool | Cost |
|------|------|------|------|
| **1. Inventory** | Count and map all `unsafe` occurrences | `verify.sh` + `grep` | ~10 s |
| **2. SAFETY comment baseline** | Run `clippy::undocumented_unsafe_blocks` and record warnings | clippy | ~30 s |
| **3. Unsound check** | Scan safe→unsafe forwarding candidates for manual review | heuristic grep | ~30 s |
| **4. Miri readiness** | Report whether Miri is available for host-testable paths | rustup/cargo probe | ~5 s |
| **5. Kani readiness** | Report whether Kani harnesses/tooling exist | file/tool probe | ~5 s |

> **User-invoked** (`/safe-unsafe-verify`). Run this before any release that touches
> unsafe code. Today this is a readiness/baseline tool, not a proof that the crate is
> sound. Step 4–5 require manual harness setup; the skill reports what's missing and
> does not auto-install tools.

---

## Quick start

```bash
# Full verification readiness pipeline
bash .agents/skills/safe-unsafe-verify/verify.sh

# Step 1 only — quick SAFETY comment grade
bash .agents/skills/safe-unsafe-verify/verify.sh --audit-only
```

---

## Step details

### Step 1 — SAFETY comment audit

The script records every `unsafe` occurrence in the HAL and runs the clippy
SAFETY-comment lint as the current baseline. Use the `unsafe-auditor` subagent for
deeper manual grading by the four-tier standard (A/B/C/D from the research report
§6.4).

The audit report is saved to `docs/review/unsafe-audit-$(date +%F).md`.

### Step 2 — Unsound encapsulation check

The most dangerous pattern: a `pub fn` takes safe parameters and passes them unchecked
into `unsafe { }`. Example of unsound:

```rust
// UNSOUND: safe caller passes any idx → UB on OOB
pub fn write_reg(idx: usize, val: u32) {
    unsafe { *((0x4401_0000 + idx * 4) as *mut u32) = val }
}
```

The script includes a deliberately conservative heuristic scanner. Treat its output
as a review queue, not a gate. It can miss macro-generated code, multiline helper
chains, trait impls, and indirect calls through private functions.

Equivalent manual command:
```bash
# Find safe fns that call unsafe fns / contain unsafe blocks
# (heuristic: `pub fn` + `unsafe {` in the same non-test function)
grep -rn 'pub fn' crates/hisi-riscv-hal/src/ \
  | grep -v '#\[' \
  | grep -v 'test' \
  | while IFS=: read -r file line rest; do
      fn_name=$(echo "$rest" | sed 's/pub fn \([a-z_]*\).*/\1/')
      if grep -A20 "^[[:space:]]*pub fn $fn_name" "$file" | grep -q 'unsafe'; then
        echo "$file:$line: $fn_name wraps unsafe"
      fi
    done
```

### Step 3 — Clippy undocumented-unsafe-blocks lint

```bash
cd crates/hisi-riscv-hal
cargo clippy --no-deps --no-default-features --features chip-ws63 \
  --target riscv32imfc-unknown-none-elf -- \
  -W clippy::undocumented_unsafe_blocks 2>&1 | \
  grep -E '(warning|error)' | grep 'undocumented'
```

> **Note**: run this on the host target for the baseline. The lint checks comment
> presence, not target-specific codegen:
> ```bash
> cargo clippy --no-deps --no-default-features --features chip-ws63 \
>   --target x86_64-unknown-linux-gnu -- \
>   -W clippy::undocumented_unsafe_blocks
> ```

### Step 4 — Miri dynamic UB detection

Miri cannot run no_std RISC-V code directly. The check works on **host-testable
standalone functions** only (pure-logic helpers, const fns, newtype methods).

```bash
cd crates/hisi-riscv-hal
cargo +nightly miri test --target x86_64-unknown-linux-gnu 2>&1
```

If Miri is not installed:
```bash
rustup +nightly component add miri
```

### Step 5 — Kani model checking (manual harness)

Kani verifies pure-logic functions exhaustively. Candidates in the HAL:

| Function | File | Why verify | Harness status |
|----------|------|------------|----------------|
| `PeripheralTransfer::wait` timeout path | `dma.rs` | Overflow-free tick computation | ❌ Not written |
| `Transfer::drop` cancel-then-quiesce | `dma.rs` | `active` poll loop terminates | ❌ Not written |
| `DmaChannelConfig::beats` bound check | `dma.rs` | `beats <= 4095` invariant | ❌ Not written |
| `PwmPeriod::from_hz` panics on zero | `pwm.rs` | `hz != 0` enforced | ❌ Not written |

To add a harness:
```rust
#[cfg(kani)]
#[kani::proof]
fn verify_beats_limit() {
    let beats: usize = kani::any();
    // assert the check in DmaChannelConfig never panics on valid input
}
```

This skill does **not** auto-write harnesses — it reports which are missing
so you can decide based on criticality.

---

## Output

After running, the skill produces a **verification readiness report**:

```
Unsafe occurrences:  N
Clippy undocumented: M warnings
Forwarding candidates: K
Miri: available / missing
Kani harnesses: present / missing
```

Combine with the stable/unstable gate: any API whose unsafe code has
**Tier D SAFETY comments** or **confirmed unsound forwardings** must remain
UNSTABLE until fixed.
