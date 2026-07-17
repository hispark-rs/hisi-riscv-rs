# WS63 RF A3 Unified Task-Context Preemption Evidence (2026-07-14)

## Scope

This evidence covers the WS63 TIMER_INT0 one-shot scheduler source, SOFT_INT0
deferred rescheduling, equal-priority round-robin preemption, and complete
floating-point trap preservation. Priority inheritance is proven separately;
this document does not claim nested-interrupt stress, Embassy integration, or
A3 completion.

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

### 2026-07-17 Idle Timed-Wake Regression

The crypto-contention HIL later exposed a separate all-blocked seam: after the
last business task slept, TIMER_INT0 made it ready but the IRQ epilogue treated
the reserved idle task as an ordinary cooperative task and refused to switch.
The same path could also enqueue idle in an ordinary ready queue. `hisi-rtos`
commit `2024e62` now keeps idle out of those queues and makes a ready business
task replace running idle at the outermost IRQ epilogue. Ordinary cooperative
user tasks remain non-preemptive.

The host gate now contains 46 unit tests plus the compile-fail capability suite.
The two focused regressions prove that an idle yield does not enqueue idle and
that a timed sleeper wakes from idle. Formatting, the 39-requirement evidence
map, RV32IMFC clippy with `-D warnings`, and the RV32IMFC release build passed.

`rtos_preemption` commit `be5034b` sleeps the adopted main task for 20 ms before
creating any dynamic worker. During that interval idle is the only eligible
task; reaching `A3_RTOS_IDLE_WAKE_OK` therefore requires TIMER_INT0 to wake main
and replace idle. A 3 MHz probe-rs raw-bin download completed with full verify;
an earlier verification failure was discarded as transport noise and did not
count as firmware evidence. The successful image then produced:

```text
A3_STAGE_IRQS_ON
A3_RTOS_IDLE_WAKE_OK
A3_STAGE_TASKS_OK
A3_STAGE_SLEEP_RETURNED
loops0=5 loops1=5 timer_irqs=102 slice_preemptions=100 software_irqs=3 fp_failures=0
A3_RTOS_PREEMPTION_OK
```

Without reflashing, 20 consecutive physical J-Link nRST captures produced both
`A3_RTOS_IDLE_WAKE_OK` and `A3_RTOS_PREEMPTION_OK` in every run, with zero panic,
exception, or failure markers.

| Artifact | SHA-256 |
| --- | --- |
| `rtos_preemption` ELF | `55c047b947912e6e6aea8019da9da87a1c2917b5a3e1940a0a6fd6b3af06d235` |
| canonical `.hisi.img` | `585a405c25910735fbb849f34ec69636c0d7e0a894bf4419af55090f44f3139f` |
| FlashPlan JSON | `d893afe84dc5230ab3d7e8ae38beb016cc096d080b64bbbc67fca1ebfdf65a98` |

## Ownership Boundary

`hisi-rtos` remains the sole maintained native backend and will eventually own
the Embassy thread-mode executor and time-driver adapter. HAL retains peripheral
async traits and low-level timer/IRQ mechanisms. `ws63-radio-sys` or its WS63 ABI
shim maps only the blob's actually referenced `LOS_`/`osal_` symbols onto
`hisi-rf-rtos-driver`; an `nm -u`/link manifest must fail CI when that bounded
symbol set grows unexpectedly. The vendor LiteOS firmware remains a differential
and HIL oracle outside the product dependency graph.

## Remaining A3 Gates

- priority inheritance with inversion HIL is complete; see
  [A3 priority-inheritance evidence](ws63-rf-a3-priority-inheritance-2026-07-14.md);
- nested runtime IRQ-bracket and timeout HIL is complete; mixed vendor-task
  scheduler stress remains;
- Embassy executor/time integration without dual ownership of TIMER0.
