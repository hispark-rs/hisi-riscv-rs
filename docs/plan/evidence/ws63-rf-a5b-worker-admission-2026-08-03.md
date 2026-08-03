# WS63 A5B Worker Admission Evidence

## Scope

This evidence closes the credential-free incremental-worker admission and
init/scan gate on two WS63 boards. It covers owner-separated task admission,
the native RTOS worker, RF initialization, one scan, bounded runner progress,
and the final `RFDBG_A5B_SCAN_PROFILE_OK` contract marker.

It does not cover association, DHCP, ping, active cancellation, late
completion, or the externally blocked pure-WPA3 gate. No user network
credential was read or recorded.

## Root Causes

The r8 profile contains two distinct task groups:

- seven vendor tasks with 24 KiB stacks;
- one Rust incremental worker with an 8 KiB stack.

Three defects were exposed in sequence:

1. The worker reserved one slot, but vendor bootstrap checked the complete
   eight-task profile against the seven remaining slots. The diagnostic
   `0x21000807` decoded to `required=8, available=7`. Backend alpha.64 split
   the seven-slot vendor contract from the eight-slot composition total.
2. The diagnostic fixture still allocated vendor stacks from the shared RF
   heap. It could provide only four 24 KiB stacks: `required=172032`,
   `available=98304`. The fixture now uses the same caller-owned
   `SchedulerStorage` and `SchedulerArena` contract as the public application.
3. The RTOS minimum stack remained globally fixed at 24 KiB, so it rejected
   the separately reserved 8 KiB worker stack. Backend alpha.65 introduced a
   heterogeneous profile floor: 8 KiB when the incremental worker is enabled,
   while every vendor reservation remains 24 KiB. The facade and applications
   derive the RTOS setting from this profile fact rather than duplicating it.

The initial fix produced resource-report schema v9. It was later superseded by
the v10 structured resource tree, which derives composition totals from child
plans and atomically dry-runs allocator reservations before touching RF
hardware. See [A5U structured resource plan evidence](
ws63-rf-a5u-structured-resource-plan-2026-08-03.md).

## Automated Verification

- 106 host tests passed with the exact WPA3 incremental-worker feature set.
- Strict target clippy and the RV32 final link passed.
- The final ELF retained 37 ROM patch entries and zero vendor relocations.
- Standalone package verification passed for backend alpha.65 and facade
  alpha.75 using their released dependencies.
- Generated Wi-Fi projects build offline and derive their RTOS minimum stack
  from the selected radio profile.

## Silicon Result

The same credential-free incremental-scan ELF was downloaded to two WS63
boards through the FlashPlan binary path at 3 MHz with full write verification.
Downloads took 90.86 and 90.71 seconds. Each board was reset through its paired
J-Link while UART capture was already active.

Both boards emitted `RFDBG_A5B_SCAN_PROFILE_OK` after RF bootstrap and scan.
The bounded runner observations were:

| Metric | Board A | Board B |
|---|---:|---:|
| scan results | 3 | 4 |
| runner invocations | 14 | 12 |
| operations completed | 2 | 2 |
| pending observations | 7 | 6 |
| budget exhausted observations | 3 | 2 |
| backend errors | 0 | 0 |
| event drops | 0 | 0 |
| blocking scan/poll calls | 0 | 0 |

This is true incremental-worker evidence. Earlier two-board init/scan runs used
the blocking bootstrap path and are retained only as a comparison baseline.

## Active Cancellation Result

A follow-up credential-free fixture used the public `WifiController::scan`
future itself: it let a real firmware scan run for 100 ms, dropped the future,
waited for native cancellation to quiesce, and then required a replacement scan
to complete. The final marker was accepted only when runner diagnostics also
reported at least one delivered cancellation and one cancelled terminal state.

The same ELF passed on both boards at 3 MHz with full verification. Downloads
took 90.54 and 90.49 seconds. Both runs observed the ordered sequence
`cancel_requested -> cancelled -> replacement completed` and emitted
`RFDBG_A5B_CANCEL_PROFILE_OK`.

| Metric | Board A | Board B |
|---|---:|---:|
| operations started | 3 | 3 |
| operations completed | 2 | 2 |
| cancellations requested | 1 | 1 |
| operations cancelled | 1 | 1 |
| runner errors | 0 | 0 |
| blocking scan/poll calls | 0 | 0 |

This closes active cancellation delivery, terminal cleanup, and operation-slot
reuse on silicon. It does not prove the stronger adversarial case where a
successful old-generation completion arrives after replacement has started.

## Worker CPU-Ownership Observation

The cancellation fixture was extended to inspect the production RTOS task
snapshot. It fails closed unless exactly one task matches the worker contract:
an 8 KiB stack with a `Budgeted` 100/200 ms periodic CPU quota. It also rejects
an observed uninterrupted run over 100 ms or any budget expiry deferred by a
scheduler lock.

The updated ELF was downloaded to both boards at 3 MHz with full verification.
Both boards retained the cancellation and replacement markers and emitted
`RFDBG_A5B_WORKER_RTOS_OK`:

| Metric | Board A | Board B |
|---|---:|---:|
| worker stack | 8 KiB | 8 KiB |
| worker CPU time | 15 ms | 11 ms |
| worker IRQ time | 1 ms | 0 ms |
| worker dispatches | 17 | 16 |
| maximum continuous run | 3 ms | 3 ms |
| maximum ready latency | 2 ms | 2 ms |
| maximum scheduler-lock hold | 1 ms | 1 ms |
| budget-lock overruns | 0 | 0 |

This calibrates CPU ownership for the credential-free init/scan/cancel workload
and confirms that the configured worker, rather than a blocking comparison
path, produced the result. It does not claim that an individual synchronous
vendor call returns within 100 ms, and this sample did not deliberately force a
quota exhaustion.

## Adversarial Stale-Generation Injection

The worker proxy previously treated any response whose `OperationId` differed
from the operation currently being polled as a protocol error. A stale terminal
response could also clear the replacement's active identity. The proxy now
discards such responses before copying scan output and keeps the replacement
active.

An opt-in diagnostic profile cancels a real scan, waits for terminal cleanup,
starts its replacement, and injects one successful old-generation worker
response immediately before the replacement's first backend poll. On both
boards the fixture observed exactly one injection and one discard, zero runner
errors, and a successful replacement scan. Both emitted:

- `RFDBG_A5B_STALE_COMPLETION_ARMED`
- `RFDBG_A5B_STALE_COMPLETION_OK`
- `RFDBG_A5B_CANCEL_PROFILE_OK`

This is an on-silicon contract injection: it proves that the production worker,
mailbox, runner, and generation boundary suppress the adversarial ordering. It
does not claim that the vendor firmware naturally produced that exact timing in
these two runs.

## Remaining Gate

A5B remains opt-in. Repeated connectivity parity still needs dedicated silicon
evidence before the incremental path can become the default.
