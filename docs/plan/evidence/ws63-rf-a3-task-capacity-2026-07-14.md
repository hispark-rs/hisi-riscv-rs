# A3 task-capacity compatibility evidence (2026-07-14)

## Scope

This evidence freezes the current compatibility capacity while the static
`SchedulerStorage<N>` design remains deferred. The scheduler owns one adopted
main slot and one idle slot, and exposes 15 dynamic task slots to the runtime
contract, for 17 total table entries.

## Host and target build evidence

- `hisi-rf-rtos-driver` host tests: 1/1 passed.
- `hisi-rtos` host tests: 37/37 passed after the behavior-preserving module
  split.
- The requirements drift check aligned 33 normative IDs.
- Kani verified `remaining_never_exceeds_capacity` (1/1 harness, 0/126
  properties failed).
- TLC completed the budget model with 498 generated / 260 distinct states and
  no invariant or liveness error.
- RV32IMFC `cargo check -Zbuild-std=core,alloc` passed.
- Host Clippy passed with `-D warnings` for both release units.
- `wifi_init_smoke --features full-init` compiled against the new
  `NoTaskSlots` error contract.

The allocation regression consumes all 15 dynamic slots in order, proves that
main and idle are skipped, and requires the next allocation to return
`NoTaskSlots`. Diagnostics report `internal=2`, `dynamic_capacity=15`,
`dynamic_used=15`, and `dynamic_free=0`. TaskId boundary tests cover the current
17-slot table and reject generation zero/out-of-table identities; a compile-time
bound prevents the low-8-bit slot encoding from truncating.

## WS63 HIL

The same runtime table passed these real-silicon markers:

```text
A3_RTOS_BUDGET_OK
A3_RTOS_PREEMPTION_OK
A3_PRIORITY_INHERITANCE_OK
A3_SCHEDULER_STRESS_OK
A3_RTOS_EMBASSY_COEXIST_OK
```

Budget enforcement observed 8 exhaustions, 7 replenishments and no scheduler
lock overrun. The post-refactor Embassy fixture observed 17 native ticks, 10
Embassy ticks, 25 timer IRQs and 34 context switches. Preemption observed 101
timer IRQs, 100 slice preemptions, 2 software IRQs and `fp_failures=0`.

The WPA2 RF image also used the 17-slot runtime without task-capacity failure.
After narrowing an overly broad MIE guard so non-blocking semaphore/mutex wake
operations cannot be dropped, the final run retained the vendor task priorities
and reached scan, WPA2 connect, DHCP, ARP and public `1.1.1.1` ping. Three earlier
runs under the invalid guard failed scan deterministically; they are negative
regression evidence for wake delivery, not radio-environment failures.

That run proves task-capacity compatibility, not deterministic radio
connectivity. The statistical reset/connect matrix remains a separate A3/RF
release gate and must not be hidden by one successful run.

A post-module-split spot check used the same WPA2-only archive and final image.
The first nRST completed scan and WPA2 association but remained at
`RF5A_DHCP_BEGIN` for the 60-second capture window. A second nRST completed scan
but returned `RF5B_WPA_CONNECT_ERR:0x00000003` before DHCP. Neither run reported
task-slot exhaustion. These two outcomes are negative connectivity evidence:
they do not invalidate the capacity regression, but they also do not prove that
the structural refactor preserved deterministic connect/DHCP behavior. Q2
per-thread tracing and the statistical reset/connect matrix must distinguish a
runtime scheduling/wake regression from an environmental or vendor-state
failure before A3 can close.

## Deferred boundary

The 17-entry table is not a permanent public storage layout. Application-owned
`SchedulerStorage<const N>`, named capacity profiles, initialization-time
reservation/quota and manifest-generated resource reports remain deferred until
the A3/RF behavior baseline is frozen.
