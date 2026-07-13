# WS63 RF A3 `hisi-rtos` Extraction Evidence (2026-07-13)

## Scope

This evidence covers the ownership move from the RF-local cooperative scheduler
to the independent `hisi-rtos` release unit. It does not claim that priority
preemption, deferred stack reclamation, or Embassy integration is complete.

## Boundary

- `hisi-rf-rtos-driver 0.1.0-alpha.2` owns the runtime-neutral task,
  semaphore, timeout, wait, and exactly-one-runtime contracts.
- `hisi-rtos 0.1.0-alpha.1` owns task slots, stacks, context switching,
  cooperative scheduling, and the registered runtime implementation.
- The RF crate no longer owns the scheduler implementation. It starts the RTOS
  explicitly with caller-provided allocation, deallocation, and monotonic-time
  resources, then uses only the driver contract.
- Allocation/deallocation and monotonic-clock reads occur outside critical
  sections. Critical sections only protect scheduler metadata.

## Build And Link Evidence

The guarded link completed with the same RF closure as the A0 baseline:

- 1,486 oracle/final RF layout sections verified;
- 5,335 vendor relocations patched;
- 37 WS63 mask-ROM patches generated;
- no unresolved vendor relocation in the final path.

Artifact SHA-256 values:

| Artifact | SHA-256 |
| --- | --- |
| `wifi_init_smoke` ELF | `13ab7aa07c2024e2f5bc873002e33027e352fc1da48e0fa0fd0a82ad66cd5de0` |
| canonical `.hisi.img` | `15cb21b5de57608f1101b31f7b515fc27e0488f874d19a621586d01ce08f9983` |
| FlashPlan JSON | `5d056a4865012151a42e75f6cc5a8f182cfb8c81bb169752661d99be6dcbde2b` |

## Silicon Evidence

After binary download and physical J-Link reset, the WS63 EVB produced all
required connectivity markers:

```text
RF1_IMAGE_OK
RF2_INIT_OK
RF3_SCAN_OK count=0x00000020
RF5B_WPA_CONNECT_OK
RF5A_DHCP_OK addr=192.168.155.2 prefix=0x00000018 router=192.168.155.1
RF5A_ARP_OK rx=0x00000004
RF5C_PING_OK rx=0x00000005
run-hw: done.
```

The WPA2 credential was injected through the build environment and is not part
of the repository or this evidence record.

## Remaining A3 Gates

- priority scheduling and TIMER_INT0/software-interrupt preemption;
- safe deferred reclamation of exited task stacks;
- FP-context and nested IRQ scheduler stress HIL;
- Embassy thread-mode executor and unique time-driver integration.

