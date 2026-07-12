---
name: pac-svd-register-access
description: Maintain hisi-hal register access through SVD/PAC truth sources. Use when changing HAL MMIO/register code, reviewing raw PAC usage, fixing check-register-access CI failures, adding or correcting SVD/PAC fields/access types, regenerating ws63-pac or bs2x-pac, deciding whether a register should be modeled in SVD versus handled in HAL, or auditing read/modify/write behavior for write-only or dynamically masked registers.
---

# PAC/SVD Register Access

Keep register facts in the SVD/PAC layer and HAL behavior in the HAL layer. When a
HAL change wants raw addresses, volatile MMIO, read/modify on write-only registers,
or whole-register RMW, first ask whether the SVD/PAC model is missing a fact.

## Workflow

1. **Classify the problem.**
   - HAL uses a raw address / volatile pointer / `w.bits(r.bits())`.
   - PAC lacks a register, field, access type, reset value, interrupt, or base.
   - SVD copied a same-name peripheral whose chip variant is not register-compatible.
   - CI fails in `check-register-access.py`.

2. **Patch the lowest truthful layer first.**
   - For WS63: edit `crates/pac/ws63-pac/ws63-svd/WS63.svd`, then regenerate `ws63-pac`.
   - For BS2X: prefer `crates/pac/bs2x-pac/bs2x-svd/tools/*` plus audited supplements, then regenerate `bs2x-pac`.
   - Never hand-edit `crates/pac/*-pac/src/lib.rs`; it is generated.

3. **Use the HAL register rules.**
   - Production HAL code must not use `read_volatile` / `write_volatile` for MMIO.
   - Production HAL code must not cast numeric MMIO addresses to pointers; model the block in SVD/PAC.
   - Write-only registers must be written with a complete target value, not read or modified.
   - Prefer field writers/readers. Use `unsafe { w.bits(...) }` only for full-register writes whose SVD field model is intentionally absent or impossible.
   - Whole-register dynamic RMW (`w.bits(r.bits() | mask)`) needs either SVD fields or an explicit allowlist rationale for dynamic bit positions.
   - DMA data-register address exports are allowed when they use PAC register accessors and are not CPU reads/writes.

4. **Validate before finishing.**
   - Run `bash .agents/skills/pac-svd-register-access/scripts/run-register-audit.sh`.
   - For generated PAC changes, also run that PAC's `regen.sh`; do not trust manual diffs.
   - For HAL behavior changes, run the relevant `cargo check`/`clippy` matrix and HIL when the change affects stable silicon behavior.

5. **Commit in dependency order.**
   - Use the `submodule-commit` skill for multi-repo landing.
   - Order: SVD submodule -> PAC submodule -> HAL submodule -> parent pointer.

## References

Read `references/register-policy.md` when implementing or reviewing a real change.
It contains the detailed checklist, examples, SVD/PAC modeling rules, and CI gate
meaning.
