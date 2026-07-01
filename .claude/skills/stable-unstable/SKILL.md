---
name: stable-unstable
description: "Manage the stable/unstable API gating in hisi-riscv-hal: add a new unstable surface, graduate one to stable (write HIL + remove the gate), or audit which items should be gated. Use when adding a driver/API, reviewing a PR that touches pub items, or checking gating compliance."
---

# Stable/Unstable API Gating Skill

This skill manages the `#[instability::unstable]` / `unstable_module!` gating in `hisi-riscv-hal`. The policy: **an API is STABLE only if a named HIL test exercises it on real WS63 silicon; everything else is UNSTABLE (gated behind the `unstable` cargo feature).**

## When to use

- **Adding a new driver or API** → it starts UNSTABLE (no HIL yet); use the "Add unstable" workflow.
- **An unstable API got a HIL test passing on silicon** → graduate it to STABLE; use the "Graduate" workflow.
- **Reviewing a PR** that adds/changes `pub` items → check gating compliance; use the "Audit" workflow.
- **Checking which items are gated** → run the scanner script.

## Quick reference: the mechanism

| Surface | Gating form | When unstable OFF | When unstable ON |
|---------|------------|-------------------|-----------------|
| `pub fn` / `pub struct` / `pub enum` (item-level) | `#[instability::unstable]` | `pub(crate)` + `#[allow(dead_code)]` (soft) | `pub` |
| `pub mod foo;` (module-level) | `unstable_module! { pub mod foo; }` | `pub(crate) mod` + `#[allow(unused)]` (soft) | `pub mod` |
| Standalone driver (nothing stable depends on it) | `unstable_driver! { pub mod foo; }` | absent (hard delete) | `pub mod` |

### Rules (critical)

1. **Inherent `impl` blocks**: UNGATED. Gate each `pub fn` inside individually. `instability` hard-deletes `impl` blocks when off → would make private helpers dead-code.
2. **`impl Drop`**: UNGATED (keeps private helpers live).
3. **Trait impls**: MAY be whole-block gated (`#[instability::unstable] impl Trait for Foo`).
4. **STABLE `pub fn` taking an UNSTABLE type as param/return**: FORBIDDEN (`private_interfaces` lint). If `write_dma` (STABLE) takes `DmaChannel`, then `DmaChannel` must also be STABLE.
5. **`async`/`embassy`**: pure feature-gates (consent-by-feature). `embassy` is ALSO `unstable`-gated (no end-to-end HIL). `async` stays STABLE.
6. **Doc comments** inside `unstable_module! { ... }`: put the `///` INSIDE the macro body (it forwards `$(#[$meta])*` to both cfg branches). A `///` outside the macro is an orphaned "unused doc comment".
7. **HIL tests** that reference UNSTABLE items: gate with `#[cfg(feature = "unstable")]` (the external integration test crate can't see `pub(crate)`).
8. **In-module host tests** (`#[cfg(test)] mod tests`): do NOT gate — the soft-gate keeps items `pub(crate)` (in-crate visible to test modules).
9. **`prelude.rs` re-exports** of UNSTABLE modules: gate the `pub use` with `#[cfg(feature = "unstable")]`.

## Workflow 1: Add a new unstable surface

When adding a new driver/API that has no HIL test yet:

1. **Item-level** (struct/fn/enum inside a file): add `#[instability::unstable]` before the `pub` keyword. If the item is chip-ws63-only, stack `#[cfg(feature = "chip-ws63")]` + `#[instability::unstable]`.
2. **Module-level** (new `pub mod foo;` in `src/lib.rs`): wrap in `unstable_module! { /// Doc... pub mod foo; }`. Put the doc INSIDE the macro.
3. **Check signature constraint**: does any STABLE `pub fn` now take/return this new UNSTABLE type? If yes → either make the type STABLE, or gate the method too.
4. **Check `prelude.rs`**: if you re-export from the new module, gate the `pub use`.
5. **Examples**: if an example uses the new surface, add `unstable` to its hal dep features.
6. **HIL tests**: if you add a HIL test for it, gate the test `#[cfg(feature = "unstable")]`.
7. **Verify**: `cargo clippy --no-default-features --features chip-ws63 --target x86_64-unknown-linux-gnu` (unstable OFF — no `private_interfaces`/`dead_code` warnings) + `cargo clippy --features chip-ws63,unstable` (ON).

## Workflow 2: Graduate (unstable → stable)

When an unstable API gets a HIL test passing on real WS63 silicon:

1. **Write the HIL test** in `tests/hil.rs` (self-contained or `hil-loopback`). Gate it `#[cfg(feature = "unstable")]` for now.
2. **Run it on silicon** (`justfile hil` with `--features chip-ws63,unstable[,async]`). It must PASS.
3. **Remove the gate**: delete `#[instability::unstable]` from the item, OR move the module out of `unstable_module!` to a plain `pub mod foo;`.
4. **Remove `#[cfg(feature = "unstable")]` from the HIL test** (it's now stable — should run in the default suite).
5. **Update docs**: optionally add `#[instability::stable(since = "0.x.0")]` to keep a "Stabilized in version X" note.
6. **Update examples**: if the example had `unstable` just for this surface, check if it still needs it (other surfaces may still be unstable).
7. **Verify**: `cargo clippy` (unstable OFF — the now-stable item must not produce `dead_code` since it's `pub`), + `cargo test` + HIL run (without `unstable` now includes the graduated test).

## Workflow 3: Audit gating compliance

To check whether an item should be STABLE or UNSTABLE:

1. **Find the item** in `src/*.rs` (grep for the struct/fn/enum/mod name).
2. **Check for a HIL test**: grep `tests/hil.rs` for a test that calls/constructs this item. Does it run on WS63 silicon? (self-contained tests run by default; `hil-loopback` tests need jumpers; `hil-rtc` is opt-in and may not have run on this board.)
3. **Verdict**: HIL test exists + runs on connected silicon → STABLE. No HIL test (or opt-in never ran) → UNSTABLE.
4. **Check the current gate**: is the item `#[instability::unstable]` or in `unstable_module!`? If STABLE but gated → it's a graduation candidate. If UNSTABLE but not gated → it's a gap (gate it).

### Scanner script

Run this to list all currently-gated items + check for ungapped unstable surfaces:

```bash
# All #[instability::unstable] items:
grep -rn '#\[instability::unstable\]' crates/hisi-riscv-hal/src/

# All unstable_module! / unstable_driver! invocations:
grep -rn 'unstable_module!\|unstable_driver!' crates/hisi-riscv-hal/src/lib.rs

# HIL tests that are gated unstable (should run only with --features unstable):
grep -n 'feature = "unstable"' crates/hisi-riscv-hal/tests/hil.rs
```

## What's currently STABLE vs UNSTABLE

See `CLAUDE.md` "Stable / Unstable API gating" section for the authoritative split, or `docs/src/explanation/policies/02-stable-unstable.md` for the full explanation. The split is audited against `tests/hil.rs` — the rule is "HIL-proven on WS63 silicon = STABLE; everything else = UNSTABLE".
