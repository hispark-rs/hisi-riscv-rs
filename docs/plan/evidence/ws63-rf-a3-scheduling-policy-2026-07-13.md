# WS63 RF A3 Scheduling Policy Evidence (2026-07-13)

## Scope

This evidence covers the first explicit task-priority contract, deferred task
stack reclamation, and the distinction between connectivity-proven cooperative
FIFO scheduling and priority selection. It does not claim timer-driven
preemption or priority inheritance.

## Released Components

- `hisi-rf-rtos-driver 0.1.0-alpha.3` adds fallible task-priority control.
- `hisi-rtos 0.1.0-alpha.2` stores LiteOS-compatible priorities (0 highest,
  31 lowest), reclaims exited stacks from another task, and exposes explicit
  `SchedulingPolicy::{Cooperative, Priority}`.
- The WS63 OSAL bridge validates the `osal_task*` handle and 0..31 priority,
  then forwards `osal_kthread_set_priority` through the driver contract.

Host tests prove priority ordering, same-priority FIFO, priority requeueing,
cooperative FIFO across priorities, cooperative yield progress, and retired
stack bookkeeping.

## Adversarial Finding

Enabling strict priority selection in the current cooperative RF backend
stalled after `device_module_init:: succ!`. A halted target snapshot showed
multiple higher-priority worker tasks ready while the initialization task was
lower priority. The vendor workload's timed-wait/ready behavior requires the
remaining LiteOS semantics: timer/software-interrupt preemption, task lock,
priority inheritance, and complete wait behavior.

Therefore `SchedulingPolicy::Cooperative` remains the default and preserves the
A0 FIFO behavior. `SchedulingPolicy::Priority` is a tested state-machine option,
not the RF default and not evidence of a preemptive RTOS.

## Build And Link Evidence

- 1,486 oracle/final RF sections verified;
- 5,335 vendor relocations patched;
- 37 WS63 mask-ROM patches generated;
- image `code_area_hash`:
  `87fe0f3907ff8986d5ac460df40e8106d3a7c5e0e3b2d0fa340ca30e4885260a`.

| Artifact | SHA-256 |
| --- | --- |
| `wifi_init_smoke` ELF | `2c78fe92aba191681f0b53fa3251d4e0eb930b090e1e9e1f92e2c6da2c3ba40a` |
| canonical `.hisi.img` | `f5d9bfd0a051531d3cfe042cf72b86176712b776c9315443a2ed3f258a3be69f` |
| FlashPlan JSON | `c8b3e9775d1eb45d5bc79f70a7cfd72767a765ce497c0ddfad68ae7ab0fb7313` |

The deterministic WPA2-Personal archive is preserved online with its source
manifest, build profile, provenance, and checksums in the
[`ws63-RF` WPA2-Personal release](https://github.com/hispark-rs/ws63-RF/releases/tag/wpa2-personal-2026-07-13).
Its archive SHA-256 is
`891c195279d768ce5664e16fecac95f353fd2217c4e3590315baa4cb6e7f25a2`.

## Silicon Evidence

The first association attempt reached init and scan but returned transient
vendor authentication error `0x1451`. A physical J-Link nRST of the unchanged
image then produced the full parity sequence:

```text
RF1_IMAGE_OK
RF2_INIT_OK
RF3_SCAN_OK count=0x00000020
RF5B_WPA_CONNECT_OK
RF5A_DHCP_OK addr=192.168.155.2 prefix=0x00000018 router=192.168.155.1
RF5A_ARP_OK rx=0x00000004
RF5C_PING_OK rx=0x00000005
```

The credential was injected through the build environment and is not stored in
the repository or evidence bundle.

## Remaining A3 Gates

- TIMER_INT0 + software-interrupt preemption and budget enforcement;
- task lock/unlock and priority inheritance;
- FP context, nested IRQ, timeout, and scheduler stress HIL;
- Embassy thread-mode executor and unique time-driver integration.

