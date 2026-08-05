# WS63 RTOS Switch-Intent Creation Evidence (2026-08-06)

## Scope

This evidence closes `A5R-F4B` for the switch-away ticket creation decision in
`hisi-rtos` commit `7cbcd4825d37bc34258f4e259fb002a85c6369a0`.

The earlier proof started after a `SwitchIntent` existed. It covered ticket
commit, cancellation, consumption, identity generations, and detached-target
ownership. This change additionally covers the decision that precedes ticket
creation: after a source task becomes non-running, the scheduler must either
commit exactly one ticket that owns exactly one detached target, or observe that
the source resumed and return `NoSwitch` without changing ready ownership.

This is a proof and parity closure, not a claim that the historical AP queue-0
stall had this single cause. The previous two-critical-section path already had
a resume check, and the new silicon run did not observe a detached pending
priority or policy mutation.

## Implementation And Proof

The production sleep, semaphore, mutex, and task-exit paths now call
`Sched::prepare_switch_away_decision` while holding one scheduler critical
section. The helper validates source identity and resume generation before it
detaches a target and commits pending ownership. `RTOS-PORT-004` maps these
production entry points directly to the host, TLA+, and Kani evidence.

Verification results:

- host tests: 83 unit tests plus the positive and compile-fail typestate UI
  cases passed on Linux;
- RV32: build and clippy with `-D warnings` passed for
  `riscv32imfc-unknown-none-elf`;
- Kani: `switch_away_prepare_or_observe_resume_is_atomic` completed 376 checks
  with zero failures;
- TLA+: the fixed model completed 6 generated/distinct states with no error;
- legacy TLA+ mode produced the expected controlled trace
  `MarkedNonRunning -> LegacyPrecheck -> IRQSwitchAndResume ->
  LegacyPrepareAfterPrecheck` and violated `PreparedSourceNotResumed`;
- GitHub Actions run
  [31033424939](https://github.com/hispark-rs/hisi-rtos/actions/runs/31033424939)
  passed the check, TLA+, and Kani jobs.

## Mechanism HIL

Each image below was built against `hisi-rtos` commit `7cbcd48`, downloaded at
3 MHz with full readback verification, and started by J-Link nRST:

| Profile | ELF SHA-256 | Result |
|---|---|---|
| Preemptive | `51e7935ab712b0256f59eaec9a93acd3d4c6efcbd2187521a6dffdcfa68c41fe` | `A3_RTOS_PREEMPTION_OK`; 100 slice preemptions, 102 timer IRQs, zero FP failures |
| Budgeted | `6ab2116519e12ce0f31086d398eb48ec9eea6308fd25466ca5ab7d2d905eb374` | `A3_RTOS_BUDGET_OK`; 8 exhaustions, 7 replenishments, zero lock overruns |
| Embassy coexistence | `15b9e8229cc0a4cedd9c11ac05cab9ff701b78a43da92acabcd3bb36ca59082a` | `A3_RTOS_EMBASSY_COEXIST_OK`; native and Embassy timers both advanced |
| Scheduler stress | `e3c9c96a47b8ec242e127d62019f38a91622259c79b942d6ac0f2c63a679c2c9` | `A3_SCHEDULER_STRESS_OK` and `A5R_RESOURCE_LIFECYCLE_OK`; no ISR-context callback |

## Dual-Board WPA3 Matrix

The unchanged-image matrix used the repository-owned pure-WPA3 SoftAP/STA
fixture. It did not use an external AP or a credential file.

| Role | Profile | ELF SHA-256 |
|---|---|---|
| AP | `pure-wpa3-softap-switch-intent-f4b-7cbcd48` | `c781c168425c986f98a72a159f845a4388604924f77a99e37f9409a113268640` |
| STA | `pure-wpa3-sta-switch-intent-f4b-7cbcd48` | `4cd2a1d9554e1445cd789447821578374fd189de878f9af2648ffa3d1e81b625` |

After both images were flashed once, paired J-Link nRST produced a 3/3
preflight and a 20/20 full matrix:

- pure-WPA3 SAE/PMF, association, DHCP, ARP, and local UDP echo passed in every
  run;
- STA echo was 200/200 packets; every AP final counter was 10 RX / 10 TX;
- authentication-response-2 timeouts: 0;
- event/control queue drops and runner errors: 0;
- allocator failures: 0;
- all 100 AP scheduler snapshots reported detached priority/policy mutation
  counters as zero;
- all 100 timer-worker snapshots advanced monotonically; its 10 sampled Ready
  states had `ready_queued=1` and `pending_target=0`;
- no panic, scheduler-contract violation, or sampled pending target occurred.

The first standalone STA boot immediately after flashing completed WPA3 and
DHCP but received 0/10 local echo replies. The subsequent paired-reset 3/3 and
20/20 matrices used the same ELF files and passed. This observation remains
recorded rather than being folded into the switch-intent proof.

## Evidence Boundary

The new proof covers the ticket creation decision; the previous proof continues
to cover the ticket lifetime. The matrix demonstrates parity under
Cooperative, Budgeted, Preemptive, Embassy, and WPA3 SoftAP/STA pressure.

It does not prove that the historical run-04/run-10 queue stall was caused by
the old creation path. The existing `recover_completed_switch_request` defense
therefore remains in place. The fixture exposes pending ownership at each
scheduler snapshot rather than a continuous maximum-pending-age metric; adding
that metric is an observability improvement, not a remaining correctness proof
obligation.
