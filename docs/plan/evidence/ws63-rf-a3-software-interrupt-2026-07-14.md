# A3 WS63 Software Interrupt Evidence (2026-07-14)

## Scope

This evidence closes only the WS63 software-interrupt routing prerequisite for
A3 deferred rescheduling. It does not claim TIMER_INT0 time slicing, execution
budgets, priority inheritance, FP stress, or Embassy integration.

## Oracle

The vendor SDK defines `SOFT_INT0_IRQN` as `LOCAL_INTERRUPT0 + 10`, where
`LOCAL_INTERRUPT0` is 26. Therefore `SYS_CTL1.SOFT_INT0` is WS63 custom local
IRQ 36, not the standard RISC-V machine-software interrupt (`mcause = 3`).

Relevant vendor files:

- `fbb_ws63/src/drivers/chips/ws63/include/chip_core_irq.h`
- `fbb_ws63/src/drivers/chips/ws63/vectors/vectors.h`
- `fbb_ws63/src/drivers/chips/ws63/arch/riscv/riscv31/interrupt.c`

The SVD now models IRQs 36 through 39 and gives `SOFT_INT_SET`/`SOFT_INT_CLR`
write-only register access and `SOFT_INT_STS` read-only access. The generated PAC
and `device.x` expose the corresponding named vectors.

## Silicon Proof

The `software_irq` example was flashed through the standard planned-bin path:

```text
hisi-fwpkg plan -> probe-rs download --binary-format bin --verify -> J-Link nRST
```

The first diagnostic intentionally enabled standard `mie.MSIE`. `SOFT_INT_STS`
became 1, but no trap arrived, disproving the initial MachineSoft hypothesis.

After enabling custom local IRQ 36 through `hisi_hal::interrupt`, the direct
trap diagnostic reported:

```text
WS63 software IRQ diagnostic
SOFT_INT_STS before set: 0x00000000
trap count: 0x00000001 mcause: 0x80000024 status after handler: 0x00000000
OK: SOFT_INT0 -> local IRQ 36
```

The final run removed the private trap and used the named `SOFT_INT0` handler
through `ws63-pac/device.x` and both `hisi-riscv-rt` WS63 interrupt tables. Two
consecutive nRST boots produced the same successful marker.

## Commits

- `ws63-svd 47d269e` -- model IRQ 36..39 and register access directions.
- `ws63-pac a436ba8` -- regenerate PAC and expose named `device.x` symbols.
- `hisi-riscv-rt ed25121` -- route IRQ 36..39 in both startup implementations.
- `ws63-examples 882e50a` -- permanent `software_irq` silicon diagnostic.

## Remaining A3 Gates

- Inject the WS63 software interrupt through a target-neutral RTOS port.
- Drive wake deadlines and time slices from TIMER_INT0.
- Prove priority inheritance and FP/nested-interrupt stress.
- Integrate the Embassy time driver/executor without dual ownership of TIMER0.
