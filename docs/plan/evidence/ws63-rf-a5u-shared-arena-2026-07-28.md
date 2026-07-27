# WS63 A5U Shared RF Arena Evidence

## Scope

This evidence measures the linker-owned heap shared by RTOS task stacks,
vendor Wi-Fi workers, the upstream supplicant, OSAL objects, and packet
metadata. It is not a supplicant-only allocation budget and it does not cover a
pure-WPA3 AP.

The diagnostic source is `hisi-rf-ws63` commit `a2ce667`; parent commit
`b31bd1706` pins it. The build used temporary environment-only credentials and
a temporary Cargo target directory. The directory was removed after testing;
no credential entered source, logs, evidence, diffs, or commits.

## WPA2 On A Transition AP

The profile used the WPA2-Personal path against an AP advertising WPA2/WPA3
transition mode. The 3 MHz download retained full verify and completed in
84.52 seconds. One nRST run completed scan, association, EAPOL authorization,
and disconnect:

```text
RFDBG_A5B_CONNECT_OK elapsed_ms=0x000000da
RFDBG_A5U_HEAP_CONNECTED arena=0x000531c0 used=0x00032e0c peak=0x00033888 live=0x000000b7 peak_live=0x000000cc alloc_fail=0x00000000 free_fail=0x00000000
RFDBG_A5B_DISCONNECT_OK elapsed_ms=0x00000035
RFDBG_A5U_HEAP_DISCONNECTED arena=0x000531c0 used=0x00031af0 peak=0x00033888 live=0x0000009c peak_live=0x000000cc alloc_fail=0x00000000 free_fail=0x00000000
RFDBG_A5B_CONNECT_PROFILE_OK
```

Decoded values:

| Metric | Connected | Disconnected |
|---|---:|---:|
| Linker-provided arena | 340,416 B | 340,416 B |
| Current used | 208,396 B | 203,504 B |
| Peak used | 211,080 B | 211,080 B |
| Live allocations | 183 | 156 |
| Peak live allocations | 204 | 204 |
| Allocation failures | 0 | 0 |
| Deallocation failures | 0 | 0 |

The earlier scan-only image exposed a 367,008-byte arena because its final ELF
layout was different. This proves that the current "all linker remainder"
arena is not a stable profile resource contract even though both samples had
ample capacity.

## WPA3 Transition Observation

The WPA3-Personal profile was run twice against the same transition-mode AP
without reflashing between attempts. Both runs completed initialization and
scan but failed before EAPOL activity. The bounded diagnostics showed no
EAPOL notifications or frames. This is a repeatable transition-SAE regression
in this setup, not pure-WPA3 evidence and not a capacity result.

No root cause is assigned yet. The failure remains separate from the external
pure-WPA3 gate and from the successful WPA2 arena measurement.

## Decision

Do not encode either observed linker remainder as the permanent arena size.
The next A5U step is an explicit, one-shot RF arena capability with:

- profile-specific minimum bytes and alignment;
- initialization before any task-stack or vendor allocation;
- `required/available` failure before hardware startup;
- one allocator owner and no silent fallback to a different region;
- a versioned report that distinguishes the shared RF arena from a
  supplicant-only budget.

Caller-owned backing storage and a final WPA3 capacity remain open until the
pure-WPA3 gate is available.
