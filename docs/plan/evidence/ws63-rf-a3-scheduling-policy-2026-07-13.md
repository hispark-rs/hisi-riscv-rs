# WS63 RF A3 Scheduling Policy Evidence (2026-07-13)

## Scope

This evidence covers the first explicit task-priority contract, deferred task
stack reclamation, scheduler-lock nesting, and the distinction between
connectivity-proven cooperative FIFO scheduling and priority selection. It does
not claim timer-driven preemption or priority inheritance.

## Released Components

- `hisi-rf-rtos-driver 0.1.0-alpha.4` adds fallible task-priority control and
  scheduler lock/unlock operations.
- `hisi-rtos 0.1.0-alpha.3` stores LiteOS-compatible priorities (0 highest,
  31 lowest), reclaims exited stacks from another task, exposes explicit
  `SchedulingPolicy::{Cooperative, Priority}`, and tracks nested scheduler
  locks per task.
- The WS63 OSAL bridge validates the `osal_task*` handle and 0..31 priority,
  then forwards `osal_kthread_set_priority` through the driver contract.

Host tests prove priority ordering, same-priority FIFO, priority requeueing,
cooperative FIFO across priorities, cooperative yield progress, retired stack
bookkeeping, nested scheduler locks, and rejection of an unbalanced unlock.

## Adversarial Finding

Enabling strict priority selection in the current cooperative RF backend
stalled after `device_module_init:: succ!`. A halted target snapshot showed
multiple higher-priority worker tasks ready while the initialization task was
lower priority. The vendor workload's timed-wait/ready behavior requires the
remaining LiteOS semantics: timer/software-interrupt preemption, priority
inheritance, and complete wait behavior. Scheduler lock/unlock is now a real
contract rather than a no-op, but it does not substitute for those semantics.

Therefore `SchedulingPolicy::Cooperative` remains the default and preserves the
A0 FIFO behavior. `SchedulingPolicy::Priority` is a tested state-machine option,
not the RF default and not evidence of a preemptive RTOS.

## Build And Link Evidence

- 1,486 oracle/final RF sections verified;
- 5,335 vendor relocations patched;
- 37 WS63 mask-ROM patches generated;
- image `code_area_hash`:
  `43bab954a50e830e5a7b1e4b18d25caa4fd5549f2f91b5f2932443bd7a793ef1`.

| Artifact | SHA-256 |
| --- | --- |
| `wifi_init_smoke` ELF | `e952c053e953e8277c23bcf7882c9f620a096e0d85413bd268c2b85a9b35a444` |
| canonical `.hisi.img` | `a65f64d260cb0d14494b4fb3fc83f8c264670533e26cee069b0aba46d0f2d3af` |
| FlashPlan JSON | `9b653dfe304b3964f86805260eccfb3d8ffb70b1cceb916d9f379c1877ae58ed` |

The deterministic WPA2-Personal archive is preserved online with its source
manifest, build profile, provenance, and checksums in the
[`ws63-RF` WPA2-Personal release](https://github.com/hispark-rs/ws63-RF/releases/tag/wpa2-personal-2026-07-13).
Its archive SHA-256 is
`891c195279d768ce5664e16fecac95f353fd2217c4e3590315baa4cb6e7f25a2`.

## Silicon Evidence

Two association attempts reached init and scan but returned transient vendor
authentication error `0x1451`. A third physical J-Link nRST of the unchanged
image then produced the full parity sequence:

```text
RF1_IMAGE_OK
RF2_INIT_OK
RF3_SCAN_OK count=0x0000001c
RF5B_WPA_CONNECT_OK
RF5A_DHCP_OK addr=192.168.155.2 prefix=0x00000018 router=192.168.155.1
RF5A_ARP_OK rx=0x00000004
RF5C_PING_OK
```

The credential was injected through the build environment and is not stored in
the repository or evidence bundle.

## Remaining A3 Gates

- TIMER_INT0 + software-interrupt preemption and budget enforcement;
- priority inheritance;
- FP context, nested IRQ, timeout, and scheduler stress HIL;
- Embassy thread-mode executor and unique time-driver integration.
