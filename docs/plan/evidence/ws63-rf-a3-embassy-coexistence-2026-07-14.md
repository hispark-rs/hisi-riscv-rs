# WS63 RF A3 Embassy Coexistence Evidence (2026-07-14)

## Scope

This evidence closes the current flat-backend gate for one `hisi-rtos`
scheduler hosting both a native stack-based task and an Embassy thread-mode
executor. It does not claim that the future `hisi-rtos-embassy` component split
or per-thread budget policy is complete.

## Design Under Test

- `hisi-rtos` owns the only `embassy-time-driver` when its `embassy` feature is
  enabled.
- `Resources::monotonic_ms` is the 1 kHz Embassy clock.
- RTOS sleep, round-robin slice and Embassy deadlines are merged before the
  application-provided `SchedulerPort` arms TIMER_INT0.
- The time-slice deadline is persistent: scheduling or changing an Embassy
  wake cannot postpone an already-running slice.
- TIMER_INT0 drains expired RTOS and Embassy waits. Wakers only make futures
  runnable; user futures are polled by the thread executor after IRQ exit.
- HAL's legacy Embassy time-driver feature is deliberately absent, preventing
  duplicate global drivers and duplicate TIMER ownership.

## Host And Cross Checks

```text
cargo test -p hisi-rtos --target aarch64-apple-darwin --features embassy
21 passed; 0 failed

cargo clippy -Zbuild-std=core,alloc -p hisi-rtos \
  --target riscv32imfc-unknown-none-elf --features embassy -- -D warnings
passed

cargo build -Zbuild-std=core,alloc -p rtos_embassy_coexist --release
passed
```

The host suite includes deadline arbitration and proves that an unrelated
deadline re-arm cannot move an active time slice later.

## Silicon HIL

Command:

```text
PORT=/dev/cu.wchusbserial11130 MONITOR=10 \
  cargo run -Zbuild-std=core,alloc -p rtos_embassy_coexist --release
```

Three consecutive WS63 nRST runs produced the same counters:

```text
A3 RTOS Embassy coexist diagnostic
native_ticks=17 embassy_ticks=10 timer_irqs=27 context_switches=34

A3_RTOS_EMBASSY_COEXIST_OK
```

The native worker sleeps for 7 ms at higher RTOS priority. Embassy futures use
11 ms and 120 ms timers on the main executor thread. Progress in both counters,
timer IRQs and context switches demonstrates shared scheduling and timer
ownership rather than two independent loops.

## Regression

After the shared-deadline change:

```text
A3 RTOS preemption diagnostic
loops0=6 loops1=5 timer_irqs=101 slice_preemptions=101 software_irqs=2 fp_failures=0
A3_RTOS_PREEMPTION_OK

A3 scheduler stress diagnostic
timeout_ok=1 irq_wake_ok=1 ran_in_handler=0 posted=1 timeout_count=1 wake_count=1 software_irqs=8
A3_SCHEDULER_STRESS_OK
```

`Flash Init Fail! ret = 0x80001341` remains the known flashboot diagnostic and
did not prevent image verification or application execution.

## Remaining Boundary

- `SchedulingPolicy` is still firmware-global. The locked future design uses
  one scheduler backend with per-thread
  `RunPolicy::{Cooperative, Budgeted, Preemptive}`.
- Budget exhaustion and forced handoff for vendor threads remain the final A3
  scheduling gate.
- The HAL time driver remains available only as a migration path; removing or
  deprecating it requires a separately reviewed HAL API change.
