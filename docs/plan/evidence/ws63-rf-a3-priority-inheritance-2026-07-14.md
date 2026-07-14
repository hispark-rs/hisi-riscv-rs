# WS63 RF A3 Priority-Inheritance Evidence (2026-07-14)

## Scope

This evidence covers the chip-neutral recursive-mutex contract, the `hisi-rtos`
priority-inheritance implementation, the WS63 OSAL adapter, and a classic
low/medium/high priority-inversion HIL. It does not claim execution-budget
enforcement, nested-interrupt stress, Embassy integration, or A3 completion.

## Behavior

- mutex ownership is recursive and only the owner may unlock;
- waiters are ordered by effective priority and receive direct handoff;
- every waiter contributes to the owner's effective priority;
- inheritance propagates through mutex-wait chains;
- timeout and handoff remove or transfer every contribution;
- changing a task's base priority does not discard active inheritance.

The WS63 `osal_mutex_*` ABI now delegates these rules to
`hisi-rf-rtos-driver`; the ABI shim no longer emulates a mutex with a binary
semaphore. A failed destroy preserves the C handle instead of losing a live
runtime allocation.

## Host And Build Evidence

- `hisi-rf-rtos-driver`: 1/1 host test and RV32IMFC clippy passed.
- `hisi-rtos`: 19/19 host tests and RV32IMFC clippy passed.
- `ws63-rf-rs` and `wifi_init_smoke --features full-init` checked successfully.
- `rtos_priority_inheritance` and `rtos_preemption` release images built.

The adversarial host cases cover duplicate same-priority donors, transitive
inheritance, waiter timeout, direct handoff with a remaining donor, and a base
priority change while inheritance is active.

## Silicon Evidence

The `rtos_priority_inheritance` image was downloaded through the FlashPlan raw
bin path, followed by physical J-Link nRST and 115200-baud UART capture. Three
consecutive runs produced:

```text
A3_PRIORITY_INHERITANCE_OK
```

The scenario makes a priority-20 task hold the mutex, a priority-10 task consume
CPU, and a priority-2 task block on the mutex. Completion proves the priority-2
waiter donated to the owner so it could run ahead of the medium task and hand
off the mutex.

The final regression run of the unified task/trap frame produced:

```text
loops0=9 loops1=8 timer_irqs=101 slice_preemptions=101 software_irqs=2 fp_failures=0
A3_RTOS_PREEMPTION_OK
```

`Flash Init Fail! ret = 0x80001341` remains an observed flashboot diagnostic on
this board and did not prevent verified image execution.

## Remaining A3 Gates

- execution-budget enforcement;
- nested-interrupt, timeout, and mixed vendor-task scheduler stress HIL;
- Embassy executor/time integration without a second scheduler or TIMER0 owner;
- pinned-blob ABI and semantic compatibility suite.
