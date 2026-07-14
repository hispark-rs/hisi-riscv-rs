# WS63 RF A3 Scheduler-Stress Evidence (2026-07-14)

## Scope

This evidence covers a real TIMER_INT0 timed semaphore wait and nested
runtime/adapter interrupt bracketing around an ISR-safe semaphore post. It proves
that a woken task runs only after the outermost IRQ exit and that a timed-out
waiter is removed cleanly. It does not claim physical nested-interrupt support,
execution-budget enforcement, Embassy integration, or A3 completion.

## Hardware Boundary

The WS63 trap entry clears machine interrupt enable and uses one shared IRQ stack.
The runtime therefore does not re-enable MIE inside a handler and does not claim
that physical interrupt nesting is supported. Doing so without per-depth trap
stack ownership would corrupt the active handler.

The tested nesting is the contract that matters to composed adapters: nested
`interrupt_enter`/`interrupt_exit` calls may occur while ISR-safe services wake
tasks, but scheduling remains deferred until the outermost exit and the common
trap epilogue.

## Scenario

`rtos_scheduler_stress` creates two zero-count semaphores:

1. a priority-2 task waits 15 ms and must receive `TimedOut`;
2. a priority-3 task waits forever on a second semaphore;
3. after both tasks block, main pends SOFT_INT0;
4. the handler enters one additional runtime IRQ bracket and posts the semaphore;
5. the task records a failure if it observes itself running while the handler's
   `IN_HANDLER` marker is still set.

## Build And Silicon Evidence

RV32IMFC release clippy passed with `-D warnings`. The image was downloaded by
the FlashPlan raw-bin path, followed by physical J-Link nRST and UART capture.
Three consecutive runs produced identical diagnostics:

```text
timeout_ok=1 irq_wake_ok=1 ran_in_handler=0 posted=1 timeout_count=1 wake_count=1 software_irqs=8
A3_SCHEDULER_STRESS_OK
```

The final unified-frame regression remained:

```text
loops0=9 loops1=8 timer_irqs=101 slice_preemptions=101 software_irqs=2 fp_failures=0
A3_RTOS_PREEMPTION_OK
```

## Remaining A3 Gates

- per-thread execution budgets and policy selection;
- Embassy executor/time integration on the same native runtime;
- pinned-blob ABI/semantic compatibility profile and RF parity rerun.
