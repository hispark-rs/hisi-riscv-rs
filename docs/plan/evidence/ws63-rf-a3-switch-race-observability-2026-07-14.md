# WS63 RF A3 Switch-Race And Observability Evidence (2026-07-14)

## Scope

This evidence closes Q2 per-thread scheduler observability and records the fix
for a ported context-switch race found with that instrumentation. It does not
close Q3 archive-bound task classification, Q4 group-quota evaluation, or A3 as
a whole.

## Failure And Root Cause

Before the fix, unchanged-image boots failed non-deterministically during scan
or WPA2 connect with a store access fault through `memcpy_s`. The poisoned source
address was `0xdeadbeef`, and a halted scheduler snapshot showed task 0, 2, 4 and
5 all marked `Running` while `current` named only task 5. This violated the
single-hart `RTOS-STATE-002` invariant and explained why RF netbuf ownership could
be released or reused while another stale execution path still referenced it.

The race was in the ported thread-switch handoff:

1. thread mode changed the current task to `Ready` or `Blocked` and detached the
   next task from the ready queue;
2. an interrupt completed a different handoff and later resumed the original
   task before it issued its explicit software-interrupt switch request;
3. the resumed task replayed the stale request, leaving multiple tasks marked
   `Running` and potentially stranding the detached target.

`hisi-rtos` commit `71577058f7feaa20b08110981de2a9f92af70f0a` adds normative
requirement `RTOS-PORT-004`. A resumed task now cancels the stale request and
restores a detached, still-`Ready` target exactly once. The bounded recovery path
checks every ready queue before enqueueing and never inserts the internal idle
slot. `Diagnostics::switch_race_recoveries` makes the path observable without
formatting or callbacks in scheduler hot paths.

## Host And Formal Evidence

- 44 host unit tests passed, including completed-handoff recovery, pending-switch
  discrimination, duplicate-enqueue prevention and idle exclusion.
- compile-fail capability tests still reject portless Budgeted/Preemptive use.
- host and RV32 all-feature Clippy passed with `-D warnings`.
- `scripts/check-requirements.py` aligned 39 requirement IDs.
- Kani proved `remaining_never_exceeds_capacity`: 126 checks, 0 failures.
- TLC explored 260 distinct scheduler-budget states with no error.

## Build And Image Evidence

The image used the preserved WPA2-Personal archive with SHA-256
`891c195279d768ce5664e16fecac95f353fd2217c4e3590315baa4cb6e7f25a2`.
The guarded link verified 1,485 RF layout sections, patched 5,334 relocations and
generated 37 mask-ROM patches. The final code-area hash was
`5c276d26993a9454e6b9a495e6df59fd2c4753aa3977a667b5ae8bde02a6382b`.

| Artifact | SHA-256 |
| --- | --- |
| `wifi_init_smoke` ELF | `d2c3518e2e2cfd48c8a2c312a1b043fef29e5f1b95d323c8b6d927657670591c` |
| canonical `.hisi.img` | `dd70ccb2d5f663ade540e1d65159de580d69bb1478fd499cfb2fe934e5190885` |
| FlashPlan JSON | `13d151ee369a53b4c837e6eb07acd96dccd2e84710275b9025a4960018eb543d` |

## Silicon Reset Matrix

The same flashed image was booted once after download and then through 20
physical J-Link nRST cycles. The passphrase was injected through the build
environment and is not stored here.

| Observation | Result |
| --- | ---: |
| scan completed | 20/20 |
| WPA2 connect completed | 20/20 |
| DHCP completed | 20/20 |
| `WLAN_AUTH_RSP2_TIMEOUT` (`0x1451`) | 0/20 |
| exception or `0xdeadbeef` poison fault | 0/20 |
| public `1.1.1.1` ICMP reply | 18/20 |
| gateway ICMP reply | 0/20 |

Every reset reported a non-zero recovery count, ranging from 6 to 16. The first
post-download snapshot reported exactly one `Running` task and that task matched
`current`; all other live tasks were `Ready` or `Blocked`. This confirms the
recovery path is exercised by the real RF workload and that the original
multi-`Running` corruption did not recur in the matrix.

The two public-ping timeouts happened after scan, association and DHCP had
already succeeded. The gateway also did not answer ICMP in this environment even
though ARP and routed public ping worked. These are separate data-path/retry
risks; they must not be rewritten as authentication failures or used to hide the
20/20 association result.

## Remaining A3 Gates

- Q3 must bind critical/worker/background/unknown task roles and policies to the
  exact radio archive hash instead of applying one policy to every vendor task.
- Q4 must measure aggregate subsystem CPU before deciding whether a group quota
  is needed. Reservation remains outside the current gate.
- Connectivity HIL must retain a statistical reset matrix and track public-ping
  reliability separately from scan/association/DHCP.
