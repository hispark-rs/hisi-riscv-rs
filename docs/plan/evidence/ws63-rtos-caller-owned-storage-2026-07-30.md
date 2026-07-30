# WS63 Caller-Owned Scheduler Storage Evidence

## Scope

This evidence closes the first WS63 caller-owned scheduler-storage migration.
It proves the ownership shape, the measured runtime-object headroom, the
unchanged linker envelope, and one complete WPA2 connectivity HIL. It does not
turn the current 15-dynamic-task profile into a permanent capacity limit, and it
does not replace the externally blocked pure-WPA3 gate.

## Failed Stack-Only Split

The first split used a 172,544-byte arena: seven 24 KiB task stacks plus 512
bytes of allocator metadata. A 1 MHz verified download booted, but
`uapi_wifi_init` returned `0xffff`. The failure marker showed:

```text
RFDBG_HEAP rtos_arena=0x0002a200 rtos_used=0x0002a1c4
rtos_free=0x0000003c rtos_allocs=0x0000001b
rtos_failures=0x0000000f rf_arena=0x0001fe00
rf_free=0x0001e798 rf_failures=0x00000000
```

The RTOS allocator therefore backs synchronization objects as well as task
stacks. Treating it as a stack-only arena exhausted that allocator while the
separate RF arena still had capacity.

## Corrected Contract

`hisi-rtos 0.1.0-alpha.17` names the backing type `SchedulerArena`; the previous
`SchedulerStackArena` name remains a deprecated migration alias. The selected
WS63 profile report `hisi-rf-resource-report/v8` separates:

| Item | Bytes |
| --- | ---: |
| dynamic task stack payload | 172,032 |
| allocator metadata | 512 |
| HIL-derived runtime-object headroom | 16,384 |
| scheduler arena | 188,928 |
| usable RF arena | 114,176 |
| aligned RF backing | 114,240 |
| combined `.hisi_shared_arenas` NOLOAD | 303,168 |

The fix reallocates 16 KiB inside the existing 296 KiB shared NOLOAD envelope;
it does not increase the SRAM reservation. The headroom is a named,
profile-versioned HIL calibration value, not an inferred universal maximum.

## Verification

- `hisi-rtos` host tests: 70 passed, including UI compile-fail coverage.
- `hisi-rf-ws63` host library tests: 72 passed.
- RV32 minimal graph, WPA2 profile, and connectivity fixture builds passed.
- Final ELF retained 37 ROM patches, contained zero vendor relocations, and
  reported `.hisi_shared_arenas` size `303168`.
- `hisi-rtos` CI run `30498079505` passed check, Kani, and TLA+ jobs.
- HIL evidence directory:
  `/private/tmp/ws63-caller-owned-storage-v8-hil-20260730-r4`.
- Artifact SHA-256:
  `02c3a47c7681d32d9556ec331d7a1ba656213ed4862e876a402119b586bdd5ae`.
- The 1 MHz download completed full write verification in 145.50 seconds.
- UART contract passed RF init, non-empty scan, WPA2 connect, DHCP, direct
  gateway ARP reply, validated UDP DNS response, and DHCP lease renewal:

```text
RF2_INIT_OK ifname=hisi-rf
RF3_SCAN_OK count=0x0000000b truncated=0x00000001
W2D_WPA2_CONNECT_OK
RF5A_DHCP_OK addr=192.168.3.14 prefix=0x00000018 router=192.168.3.1
RF5C_LOCAL_DATA_PATH_OK arp_reply=0x00000001
RF5C_PUBLIC_DNS_OK target=223.5.5.5 responses=0x00000001
A4_DHCP_RENEW_OK client=0x00000001 server=0x00000001
```

The reset-matrix contract summary was `{"pass": 1}`. The retained local
credential file was not copied into the evidence or repository.

## Repeated WPA2 Calibration

A follow-up final-image matrix added a fail-closed resource calibration
contract. Every successful DHCP renewal emits both the compiled profile
contract (`RFDBG_RESOURCE`) and the live scheduler/RF heap watermark
(`RFDBG_HEAP`). The classifier rejects missing markers, arena mismatches,
invalid used/free/peak relationships, or any allocation failure.

- Evidence directory:
  `/private/tmp/ws63-runtime-resource-calibration-20260730-r2`.
- Artifact SHA-256:
  `911d44eb0e47df352da2796a7d489040393e39aa4c8890682d21b174d56ed5d0`.
- One 1 MHz download completed full verification in 144.53 seconds; all 20
  samples then used the unchanged image and J-Link nRST.
- All 20 runs completed WPA2 association, DHCP, direct gateway ARP, DHCP
  renewal, and the strict resource contract with zero authentication-response-2
  timeouts, zero RTOS allocation failures, and zero RF allocation failures.
- Scheduler peak usage ranged from 172,616 to 172,660 bytes out of 188,928,
  leaving at least 16,268 bytes.
- RF peak usage ranged from 50,392 to 59,672 bytes out of 114,176, leaving at
  least 54,504 bytes.
- The public UDP DNS observation was 17/20. Runs 7, 10 and 12 still completed
  the local gateway and resource gates but received no response from either
  public DNS target. They remain `public_dns_failure` connectivity evidence and
  are not rewritten as allocator or local-data-path failures.

`hisi-rf-ws63 0.1.0-alpha.60` therefore marks only the
`wifi-wpa2-smoltcp` runtime resources calibrated. The WPA3 profile remains
uncalibrated until its separate silicon gate is available.

## Remaining Boundary

`SchedulerStorage<15>` now makes the dynamic-task quota caller-owned and
`SchedulerArena<N>` makes scheduler allocation bytes caller-owned. The internal
TCB representation is still fixed to the current maximum. The WPA2 profile has
repeated-silicon calibration; WPA3 and future profiles require their own
evidence rather than inheriting that bit. Future manifest-generated storage may
replace this shape without weakening the current initialization-time admission
or one-owner contract.
