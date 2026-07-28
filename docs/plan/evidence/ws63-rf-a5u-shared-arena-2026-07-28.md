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

## Closure

Neither observed linker remainder was encoded as the permanent arena size.
The release chain now provides an explicit, one-shot RF arena capability with:

- profile-specific minimum bytes and alignment;
- initialization before any task-stack or vendor allocation;
- `required/available` failure before hardware startup;
- one allocator owner and no silent fallback to a different region;
- a versioned report that distinguishes the shared RF arena from a
  supplicant-only budget.

The selected profile reserves 296 KiB with 16-byte alignment. `hisi-riscv-rt
0.5.7` fixes the WS63 ACPU SRAM fact at 544 KiB, places the arena in a dedicated
NOLOAD section, reserves a 32 KiB radio main stack, clears the arena before
D-cache enable, and rejects overlap at link time. `hisi-rf-ws63
0.1.0-alpha.28` owns the one-shot claim/install contract and its actionable
admission diagnostic; `hisi-rf 0.1.0-alpha.38` exposes both through the public
facade.

The independently linked final image had:

| Region | Start | Size / end |
| --- | ---: | ---: |
| `.bss` | `0x00A11D30` | `0x000200DC` |
| `.hisi_shared_arenas` | `0x00A31E40` | `0x0004A040`, ending `0x00A7BE80` |
| gap before stacks | `0x00A7BE80` | `0x00000C80` |
| `.stacks` | `0x00A7CB00` | `0x00009400` |

The section is 64 bytes larger than the usable 296 KiB arena because it also
contains ownership/profile metadata. A 3 MHz probe-rs download retained full
verify and completed in 92.75 seconds. After nRST, the same image completed
bootstrap, initialization and a non-empty scan, ending in
`RFDBG_A5B_SCAN_PROFILE_OK`; event drops and backend errors were both zero.

Caller-owned backing storage and pre-init memory admission are therefore
closed. A pure-WPA3 run is still required to calibrate SAE-path peak usage and
to close WPA3 reliability, but that external gate does not change the
296 KiB profile contract without a new measured profile revision.

## Resource-Shortage Diagnostic Parity

The production admission path was also exercised with zero bytes of caller
storage. QEMU and real WS63 emitted the same allocation-free
`hisi-rf-error/v2` document before RF power-up or blob initialization:

```text
RFDBG_A5U_TYPED_ERROR_JSON {"schema":"hisi-rf-error/v2","code":"resource.unavailable","stage":"runtime","action":"provide_resources","backend_code":1462939650,"profile_revision":"ws63-wifi-2026-07-26","trace":[{"kind":"resource_required","value":303104},{"kind":"resource_available","value":0}],"trace_truncated":false,"docs":"errors-resource-unavailable"}
RFDBG_A5U_TYPED_ERROR_OK code=resource.unavailable stage=runtime action=provide_resources
```

This closes QEMU/HIL stable-class parity for arena resource shortage without
requiring an AP, RF activity, or credentials. The WS63 run retained full verify
at 3 MHz and completed download in 2.28 seconds. Backend CI/publish runs were
`30320681798`/`30320859956`; facade CI/publish runs were
`30320968688`/`30321264555`. Association rejection, first-EAPOL timeout,
cancellation and backend timeout remain separate open parity cases.
