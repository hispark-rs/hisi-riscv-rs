# WS63 RF Blob Runtime Compatibility Plan

## Scope And Ownership

This plan defines the bounded compatibility layer between a pinned WS63 radio
archive and the native Rust runtime. It does not make `hisi-rtos` a LiteOS clone
and does not add a LiteOS backend.

The facts are deliberately separated:

- `hisi-rtos` behavior comes from its Rust API, Embassy and embedded-Rust needs,
  host-deterministic tests, and future cross-chip runtime architecture.
- WS63 RF compatibility comes from the blob ABI, its exact archive hash, the
  matching fbb_ws63 LiteOS behavior/disassembly, and RF HIL.

The dependency direction is:

```text
WS63 blob -> ws63-radio-sys -> WS63 compatibility adapter
                                      |
                                      v
                         hisi-rf-rtos-driver -> hisi-rtos
Application / Embassy --------------------------^
```

`hisi-rf-rtos-driver` owns small chip-neutral capabilities. The WS63 adapter owns
`LOS_*`/`osal_*` calling conventions, priority and tick conversion, direct-handoff
profile, callback context, and compatibility tests. Vendor LiteOS remains an
oracle outside the product dependency graph.

## Three Evidence Gates

### ABI Contract

Owned by `ws63-radio-sys`:

- archive SHA-256 and build identity;
- `llvm-nm -u` required-symbol manifest;
- RV32 function signatures, callback ABI, variadic/`va_list` boundaries;
- structure size/alignment/offset assertions;
- ROM symbols, relocation inputs, and linker requirements.

`required-symbols.txt` is version controlled. A new unresolved symbol or archive
hash must fail CI and force profile review; the compatibility surface never grows
silently.

### Semantic Contract

Owned by the WS63 compatibility adapter. LiteOS scenarios are rewritten as Rust
behavior assertions, not copied as `LOS_*` APIs and not presented as generic RTOS
semantics. A deterministic harness uses actions such as `Spawn`, `Yield`,
`AdvanceTime`, `LockScheduler`, `SemWait`, `SemPost`, `EnterIrq`, and `ExitIrq`,
and observations such as `Running`, `Blocked`, `TimedOut`, `Granted`,
`PreemptionDeferred`, and `ContextSwitched`.

Differences are absorbed by typed adapter state: `VendorPriority`, tick-to-duration
conversion, deferred callback workers, and ABI return-code mapping. A mismatch is
first fixed in the adapter; generic runtime behavior changes only when it is a
sound cross-platform capability.

### Silicon Contract

Owned by RF HIL: init, scan, connect, WPA, DHCP, ARP, ping, reset matrices,
IRQ/scheduler stress, and statistical failure rate. Host semantic tests cannot
substitute for ABI closure or silicon evidence.

## Initial Scenario Set

Only capabilities referenced by the pinned blob are enabled.

- Scheduler: nested lock deferral, highest-priority unlock, explicit numeric
  priority mapping, ready-task preemption, same-priority FIFO yield, zero-tick
  vendor yield, and same-priority-only time slicing.
- Semaphore: count/block, wait forever, timeout queue removal, direct handoff,
  highest-priority waiter, ISR wake at IRQ exit, and non-stealable grants.
- Mutex: owner-only recursive unlock, highest-priority inheritance, multiple
  donors, transitive donation, timeout restoration, and direct handoff.
- Task/timer: one-shot/rearm/cancel, timeout rounding/wrap, callback context,
  task argument/stack alignment/return-to-exit, handle generation, stack
  reclamation, and full GPR/FPR/FCSR preservation.

Queues, events, or software timers enter this list only when `nm -u` and call-site
evidence prove the archive uses them. LiteOS shell/POSIX breadth is out of scope.

## Oracle And Attribution

Open LiteOS V2 BSD-3 demos may be translated into Rust assertions with source and
attribution comments. WS63 vendor 5.10 source, final map, and disassembly are used
as behavior-only oracles unless their incremental license permits copying. Each
test records oracle path/version, blob SHA-256, and one or more evidence labels:
`OpenSourceBehavior`, `VendorSourceBehavior`, `DisassemblyConfirmed`, or
`BlobHilConfirmed`.

The compatibility profile binds to an archive hash, not the broad name
"LiteOS 5.10". A changed archive regenerates symbols, reopens semantic review,
and reruns RF HIL.

## Milestones

1. **CABI0 Manifest:** generate archive hash, required symbols, ABI layout and
   symbol-to-capability mapping.
2. **CSEM0 Harness:** implement deterministic scheduler/semaphore/IRQ scenarios,
   prioritizing paths related to the former `WLAN_AUTH_RSP2_TIMEOUT` risk.
3. **CSEM1 Mutex/task:** add only blob-used mutex, timer, queue, event and task
   lifecycle scenarios.
4. **CHIL0 Parity:** run unchanged-image reset matrices and init/scan/connect/ping;
   compare failures and traces with the vendor firmware oracle.
5. **CCI0 Gate:** make archive hash, symbol closure, semantic profile, and HIL
   evidence explicit release inputs.

Embassy integration remains native `hisi-rtos` work. It shares the runtime with
vendor threads and never starts a second LiteOS scheduler.
