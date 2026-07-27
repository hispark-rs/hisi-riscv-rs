# WS63 RTOS A5R Resource-Lifecycle Evidence (2026-07-28)

## Scope

This evidence covers the `RTOS-WAIT-003` resource-lifecycle and
`RTOS-WAIT-004` wait-cancellation contracts on real WS63 silicon. It exercises
stale and duplicate resource handles, destroy-with-waiter/owner rejection,
cancel-after-direct-grant, and stale task identity after slot reuse.

It does not graduate every HIL-marked RTOS requirement or replace the existing
preemption, budget, Embassy, FPU, and RF parity evidence.

## Regression Found

The first instrumented run reached the existing scheduler marker but reported:

```text
A3_SCHEDULER_STRESS_OK
A5R_RESOURCE_LIFECYCLE_FAIL flags=262127 expected=262143
```

The missing bit identified a cancelled forever semaphore waiter that was
reported as `Acquired`. The scheduler had correctly revoked the direct grant,
but `Semaphore::down_timeout(u32::MAX)` discarded the recorded grant result and
returned success unconditionally.

`hisi-rtos` commit `1a7aecb` makes the forever-wait path return the actual grant
state after resumption. It also binds `A5R_RESOURCE_LIFECYCLE_OK` to
`RTOS-WAIT-003` and `RTOS-WAIT-004` in the machine-readable requirement
manifest. The WS63 scenario is in `ws63-examples` commit `cceba4b`.

## Silicon Matrix

The corrected release image was downloaded once through the FlashPlan raw-bin
path with full verify at 3 MHz. The same image then ran through 20 consecutive
J-Link nRST cycles without reflashing.

All 20 runs produced:

```text
timeout_ok=1 irq_wake_ok=1 ran_in_handler=0 posted=1 timeout_count=1 wake_count=2 software_irqs=14
A3_SCHEDULER_STRESS_OK
A5R_RESOURCE_LIFECYCLE_OK
```

Result:

- resource lifecycle marker: 20/20;
- existing scheduler-stress marker: 20/20;
- duplicate/stale handle acceptance: 0;
- destroy-with-waiter/owner acceptance: 0;
- cancelled direct grant reported as acquired: 0.

All 20 UART captures were byte-identical, with SHA-256
`2724389a28caddea7b45da611a42225d6908e098e2011f911da756ee5086cc4f`.

## Evidence Boundary

This closes the new resource-lifecycle and cancellation HIL gap. A5R-F5 remains
partially open because the other HIL-marked requirements retain their own
mechanism-specific evidence and graduation gates.
