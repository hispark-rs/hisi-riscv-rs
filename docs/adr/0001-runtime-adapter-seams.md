# ADR 0001: Split hisi-riscv-rt Into a Core Runtime Facade and Chip Startup Adapters

Date: 2026-07-04

## Status

Accepted

## Context

`hisi-riscv-rt` started as the WS63 runtime: reset assembly, trap dispatch,
linker scripts, WS63 `device.x`, and the optional WS63 boot header lived at the
crate root. BS2X examples later reused parts of that implementation by supplying
their own `memory.x` and PAC `device.x`, but the exported linker script was still
named `ws63-link.x`. Future Hi3322 work has a different platform shape again:
the vendor SELiteOS path uses TES/TEE CSRs (`tmtvec`, `tmstatus`, `tmedeleg`,
`tmesvec`), CLIC setup, and a different memory partition model.

Keeping those facts in one flat runtime module makes the Interface shallow:
callers see a chip-neutral crate name but must know which WS63 facts are actually
embedded inside it. That harms locality when adding another chip.

## Decision

Keep the public crate name `hisi-riscv-rt`, but split its implementation into:

- `rt_core`: chip-neutral facade over `riscv-rt` entry attributes and shared
  runtime contracts.
- chip startup adapters: WS63 owns its reset/linker/header facts; BS2X is an
  explicit compatibility adapter; Hi3322 remains a documented placeholder until
  PAC, linker, image packaging, and board validation exist.
- image packaging remains outside the core runtime, owned by chip adapters and
  tools such as `hisi-fwpkg`.

Downstream binaries use `-Thisi-riscv-link.x`. The old `ws63-link.x` name remains
as a temporary compatibility alias.

## Consequences

- WS63 behavior remains stable while its assumptions become local to the WS63
  adapter.
- BS2X no longer depends on a misleading linker-script name, and its current
  partial reuse is documented as compatibility rather than full validation.
- Hi3322 cannot accidentally be mapped onto the WS63 reset path by feature flag.
- A future `riscv-rt` `_start` experiment can happen behind a feature without
  changing the public crate Interface.
