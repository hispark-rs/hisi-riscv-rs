# WS63 A5B Worker Admission Evidence

## Scope

This evidence closes the credential-free incremental-worker admission and
init/scan gate on two WS63 boards. It covers owner-separated task admission,
the native RTOS worker, RF initialization, one scan, bounded runner progress,
and the final `RFDBG_A5B_SCAN_PROFILE_OK` contract marker.

It does not cover association, DHCP, ping, active cancellation, late
completion, or the externally blocked pure-WPA3 gate. No user network
credential was read or recorded.

## Root Causes

The r8 profile contains two distinct task groups:

- seven vendor tasks with 24 KiB stacks;
- one Rust incremental worker with an 8 KiB stack.

Three defects were exposed in sequence:

1. The worker reserved one slot, but vendor bootstrap checked the complete
   eight-task profile against the seven remaining slots. The diagnostic
   `0x21000807` decoded to `required=8, available=7`. Backend alpha.64 split
   the seven-slot vendor contract from the eight-slot composition total.
2. The diagnostic fixture still allocated vendor stacks from the shared RF
   heap. It could provide only four 24 KiB stacks: `required=172032`,
   `available=98304`. The fixture now uses the same caller-owned
   `SchedulerStorage` and `SchedulerArena` contract as the public application.
3. The RTOS minimum stack remained globally fixed at 24 KiB, so it rejected
   the separately reserved 8 KiB worker stack. Backend alpha.65 introduced a
   heterogeneous profile floor: 8 KiB when the incremental worker is enabled,
   while every vendor reservation remains 24 KiB. The facade and applications
   derive the RTOS setting from this profile fact rather than duplicating it.

The resulting resource-report schema is v9. The physical storage owner,
profile minimum and runtime configuration now agree for this fixture. A future
structured resource tree must derive composition totals from child plans and
atomically dry-run allocator reservations before touching RF hardware.

## Automated Verification

- 106 host tests passed with the exact WPA3 incremental-worker feature set.
- Strict target clippy and the RV32 final link passed.
- The final ELF retained 37 ROM patch entries and zero vendor relocations.
- Standalone package verification passed for backend alpha.65 and facade
  alpha.75 using their released dependencies.
- Generated Wi-Fi projects build offline and derive their RTOS minimum stack
  from the selected radio profile.

## Silicon Result

The same credential-free incremental-scan ELF was downloaded to two WS63
boards through the FlashPlan binary path at 3 MHz with full write verification.
Downloads took 90.86 and 90.71 seconds. Each board was reset through its paired
J-Link while UART capture was already active.

Both boards emitted `RFDBG_A5B_SCAN_PROFILE_OK` after RF bootstrap and scan.
The bounded runner observations were:

| Metric | Board A | Board B |
|---|---:|---:|
| scan results | 3 | 4 |
| runner invocations | 14 | 12 |
| operations completed | 2 | 2 |
| pending observations | 7 | 6 |
| budget exhausted observations | 3 | 2 |
| backend errors | 0 | 0 |
| event drops | 0 | 0 |
| blocking scan/poll calls | 0 | 0 |

This is true incremental-worker evidence. Earlier two-board init/scan runs used
the blocking bootstrap path and are retained only as a comparison baseline.

## Remaining Gate

A5B remains opt-in. Active cancellation, late-completion suppression, repeated
connectivity parity, and the 100 ms CPU-ownership policy still need dedicated
silicon evidence before the incremental path can become the default. Host tests
cover generation-safe cancellation, but this run did not request cancellation
and therefore cannot close that HIL gate.
