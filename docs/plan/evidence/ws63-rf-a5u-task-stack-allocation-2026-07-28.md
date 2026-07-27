# WS63 A5U Task Stack Allocation Evidence

## Scope

This run measured the dynamic stack allocations visible after upstream hostap
initialization and one completed WS63 scan. It did not attempt association and
does not provide pure-WPA3 evidence.

The image used:

- parent base: `ec74c5665c333658d8856789a7cf292940539e09`;
- `hisi-rtos`: `a580c1e9a3edcfe840e8e5469fb1d04c13431a73`;
- `ws63-examples`: `985aee21e7f2ee7cb4092400e891f2defc736d04`;
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

## Boundary

This closes observability of the actual allocation request selected by the RTOS.
It does not close:

- caller-owned task-stack storage;
- stack high-water measurement;
- connect-time maximum live task count;
- supplicant arena ownership or size;
- memory-profile admission before hardware initialization.

Those remain A5U work. The unavailable pure-WPA3 AP gate remains external and
unchanged.
