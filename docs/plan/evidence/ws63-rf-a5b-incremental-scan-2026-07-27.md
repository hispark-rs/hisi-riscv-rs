# WS63 A5B Incremental Init/Scan Evidence

## Scope

This evidence covers the opt-in A5B incremental backend on real WS63 silicon:

- blocking RF/bootstrap ownership transfer;
- incremental initialize acknowledgement;
- callback-to-runner wake propagation;
- immediate local continuation after bounded work;
- one credential-free scan;
- bounded runner and queue diagnostics.

It does not cover connect, disconnect, DHCP, ping, or pure WPA3. The tested scan
does not consume or emit AP credentials, SSIDs, BSSIDs, or frame contents.

## Build And Transport

- `hisi-rf-core`: commit `e0a3de5`
- `hisi-rf-ws63`: commits `e4b381d` and `0cedcae`
- target: `riscv32imfc-unknown-none-elf`
- profile: release, LTO enabled by the repository profile
- image semantics: `hisi-fwpkg` planned full image
- transport: probe-rs binary download at 3 MHz
- verification: full write verify
- reset/capture: J-Link nRST followed by UART0 at 115200 baud
- download duration: 75.66 seconds

## Silicon Result

The same boot completed all RF bootstrap stages and emitted:

```text
RFDBG_A5B_BOOTSTRAP_OK elapsed_ms=0x00000148
RFDBG_A5B_INITIALIZE_OK elapsed_ms=0x00000024
RFDBG_A5B_SCAN_OK elapsed_ms=0x0000062b count=0x0000000a truncated=0x00000000
RFDBG_A5B_EVENT pending=0x00000002 high_water=0x00000002 dropped=0x00000000
RFDBG_A5B_CONTROL pending=0x00000000 high_water=0x00000001
RFDBG_A5B_RUNNER run=0x00000006 waits=0x00000006 wake=0x00000006 immediate=0x00000004 operations=0x00000002 completed=0x00000002 pending=0x00000001 exhausted=0x00000001 errors=0x00000000
RFDBG_A5B_SCAN_PROFILE_OK
```

The six runner calls took 5, 6, 12, 7, 12, and 6 ms. A 10 ms diagnostic
`WorkBudget` therefore produced one expected bounded exhaustion and a fair
immediate continuation; the operation completed without a backend error. The
earlier 2 ms provisional value was not supported by silicon timing.

## Regression Closure

Two correctness defects were required to reach this result:

1. A terminal timer deadline was added to the visible wait set before deciding
   whether the backend had immediate local work. This masked an empty backend
   wait set and deferred owned work until the timer fired.
2. WS63 callbacks woke the legacy runtime semaphore but did not publish the
   incremental backend signal, so the Embassy wait bridge could sleep through
   scan and association events.

The LTO build also exposed that strong assembler aliases for absolute mask-ROM
addresses are unsafe for preserved `R_RISCV_CALL_PLT` relocations. The adapter
now emits linker-script `PROVIDE` definitions again.

## Automated Gates

- `hisi-rf-core` CI `30260136814`: passed.
- `hisi-rf-ws63` CI `30260695179`: passed.
- The WS63 run covers package/lock verification, WPA2/WPA3 blocking and
  incremental host/RV32 profiles, and stock-rust-lld final links for plain,
  bootstrap, and incremental-scan firmware on Linux, macOS, and Windows.

## Remaining Gate

A5B remains opt-in. Incremental connect/disconnect/poll timing and parity still
need silicon evidence before the default blocking backend can be reconsidered.
The available transition-mode AP cannot satisfy the separate pure-WPA3
SAE+PMF gate, which remains externally blocked.
