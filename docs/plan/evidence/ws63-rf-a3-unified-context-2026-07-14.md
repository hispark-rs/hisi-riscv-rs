# WS63 RF A3 Unified Task-Context Preemption Evidence (2026-07-14)

## Scope

This evidence covers the WS63 TIMER_INT0 one-shot scheduler source, SOFT_INT0
deferred rescheduling, equal-priority round-robin preemption, and complete
floating-point trap preservation. It does not claim priority inheritance,
nested-interrupt stress, Embassy integration, or A3 completion.

## Oracle And ABI

WS63 LiteOS is used only as a behavior and assembly oracle. It is not a runtime
dependency and will not become a maintained backend. The checked vendor
artifacts are:

- `Huawei_LiteOS/arch/riscv/include/arch/task.h` for `TaskContext`;
- final `ws63-liteos-app.asm` symbols `TrapVector`, `ArchTaskSchedule`,
  `SaveRunTask`, `SwitchNewTask`, and `ArchTaskStackInit`;
- final map evidence showing the vendor `runstop.S.obj` implementation.

The resulting contract is one 272-byte frame: `mstatus`, `mepc`, `tp`, original
`sp`, GPRs, all 32 FPRs, `fcsr`, and three reserved words. Interrupt entry fills
the complete frame. Cooperative switching uses the same layout while only
refreshing ABI-preserved GPR/FPR fields. Fresh tasks receive a synthetic frame.
Every restore path exits through `mret`.

`hisi-rtos::TaskContext` has compile-time size and offset assertions. Both the
default WS63 startup and `riscv-rt-start-experiment` assembly use the same
offsets. Vendor-only instructions and undocumented CSRs were not copied.

## Register Truth Source

Silicon showed that TIMER EOI is write-one-to-clear: reads return zero and do
not acknowledge the source. The WS63 SVD now marks both `EOI_REN` and per-channel
`TIMER_EOI` registers write-only, so the PAC no longer exposes `read()` or
`modify()` for them. HAL alarm and async paths issue explicit writes and clear
the interrupt-controller pending bit.

## Host And Build Evidence

- `hisi-rtos`: 14/14 host unit tests passed.
- `hisi-rtos`: RV32IMFC `-Zbuild-std=core,alloc` check passed.
- `hisi-riscv-rt`: default WS63 and `riscv-rt-start-experiment` checks passed.
- `hisi-hal`: WS63 `rt,unstable` check passed against the regenerated PAC.
- `rtos_preemption`: release build passed.

## Silicon Evidence

The `rtos_preemption` image was downloaded through the FlashPlan raw-bin path,
followed by physical J-Link nRST and 115200-baud UART capture. Three consecutive
runs produced the same result:

```text
A3_STAGE_SLEEP_RETURNED
loops0=6 loops1=5 timer_irqs=101 slice_preemptions=101 software_irqs=2 fp_failures=0
A3_RTOS_PREEMPTION_OK
```

The two worker tasks retain distinct values in caller-saved `ft0`; TIMER_INT0
deliberately overwrites `ft0` in the handler. `fp_failures=0` therefore proves
that the interrupted full FPR frame is restored, rather than merely showing that
both tasks made progress.

## Ownership Boundary

`hisi-rtos` remains the sole maintained native backend and will eventually own
the Embassy thread-mode executor and time-driver adapter. HAL retains peripheral
async traits and low-level timer/IRQ mechanisms. `ws63-radio-sys` or its WS63 ABI
shim maps only the blob's actually referenced `LOS_`/`osal_` symbols onto
`hisi-rf-rtos-driver`; an `nm -u`/link manifest must fail CI when that bounded
symbol set grows unexpectedly. The vendor LiteOS firmware remains a differential
and HIL oracle outside the product dependency graph.

## Remaining A3 Gates

- priority inheritance with inversion HIL;
- nested-interrupt, timeout, and mixed vendor-task scheduler stress;
- Embassy executor/time integration without dual ownership of TIMER0.
