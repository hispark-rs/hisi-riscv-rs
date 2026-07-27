# WS63 A5U Task Stack Allocation Evidence

## Scope

This run measured the dynamic stack allocations visible after upstream hostap
initialization and one completed WS63 scan. It did not attempt association and
does not provide pure-WPA3 evidence.

The image used:

- parent base: `ec74c5665c333658d8856789a7cf292940539e09`;
- `hisi-rtos`: `a580c1e9a3edcfe840e8e5469fb1d04c13431a73`;
- `ws63-examples`: task-stack image
  `985aee21e7f2ee7cb4092400e891f2defc736d04`; heap-metrics image
  `6778cae`;
- profile: upstream hostap WPA3-capable transition profile, init/scan-only;
- transport: probe-rs planned-bin download at 3 MHz with full verify, followed
  by J-Link nRST and UART capture.

## Result

The plain Cargo image passed the final-link gates before download:

- 37 ROM patch entries;
- zero vendor relocations;
- upstream supplicant markers present;
- legacy vendor supplicant boundary absent from the selected closure.

The 3 MHz download completed with full verify in 91.68 seconds. On silicon the
image emitted `RF1_IMAGE_OK`, `RF2_INIT_OK`, a non-empty scan result, and
`W2D_NATIVE_RUNNER_RX_READY`.

At the post-scan stable point, the RTOS snapshot reported:

| Slot | Role at this gate | Stack allocation |
|---:|---|---:|
| 0 | adopted main | 0 |
| 1 | internal idle | 0 |
| 2 | public radio runner | 24 KiB |
| 3-6 | live vendor/runtime workers | 24 KiB each |

Five live dynamic tasks therefore accounted for 120 KiB of allocated task
stacks at the scan gate. Every dynamic task used the runtime's current 24 KiB
minimum. The profile continues to reserve six dynamic slots because connect may
create or require one additional worker; this scan-only sample is not evidence
that 120 KiB is the final connect-time envelope.

The follow-up image exposed the existing shared RF allocator metrics at the same
post-scan point:

| Metric | Value |
|---|---:|
| Managed arena | 367,008 bytes |
| Current used | 183,696 bytes |
| Peak used | 193,764 bytes |
| Live allocations | 156 |
| Peak live allocations | 180 |
| Allocation failures | 0 |
| Deallocation failures | 0 |

This is the shared vendor/supplicant/OSAL heap, not a supplicant-only arena.
The scan sample leaves 173,244 bytes free in the allocator's free list, but that
is neither a largest-contiguous-allocation guarantee nor a connect/SAE/EAPOL
peak. The arena must not be reduced to the observed peak without the remaining
profile HIL matrix.

## Boundary

The first run closed observability of the actual allocation request selected by
the RTOS. A follow-up implementation and commit-state HIL closed pre-init task
stack admission:

- `hisi-rf-rtos-driver 0.1.0-alpha.17` defines the v1.4 owner-bound slot and
  stack reservation contract;
- `hisi-rtos 0.1.0-alpha.13` preallocates all requested stacks outside the
  scheduler critical section, atomically publishes stacks with task slots, and
  rolls back every allocation if either phase fails;
- `hisi-rf-ws63 0.1.0-alpha.26` reserves six 24 KiB stacks before touching
  radio hardware and reports the exact 147,456-byte commitment;
- `hisi-rf 0.1.0-alpha.36` selects that backend through the public facade.

The dependency chain is pinned by parent commit `c23c2b45e`. Its credential-free
bootstrap image downloaded at 3 MHz with full verify in 75.38 seconds, completed
all initialization stages, and emitted:

```text
RFDBG_BOOTSTRAP_PROFILE_OK
A5U_TASK_STACK_ADMISSION_OK bytes=0x00024000 reserved=0x00000002
```

The two remaining reservations show that initialization consumed four of the
six promised slot/stack pairs without losing the connect-time envelope.

This does not close:

- caller-owned task-stack storage;
- stack high-water measurement;
- connect-time maximum live task count;
- separation and caller ownership of the shared RF/supplicant arena;
- connect/SAE/EAPOL heap peak and fragmentation envelope;
- whole-profile memory admission beyond task slots and stacks.

Those remain A5U work. The unavailable pure-WPA3 AP gate remains external and
unchanged.
