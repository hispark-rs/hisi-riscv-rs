# WS63 A5U Structured Resource Plan Evidence

## Scope

This evidence closes the resource-model consistency gap exposed after the
incremental worker was added. It covers child-derived task requirements,
heterogeneous stack allocation, atomic admission, allocator diagnostics, final
ELF layout consistency, and a credential-free init/scan HIL.

It does not claim pure-WPA3 parity, WPA3 resource calibration, or repeated
connectivity reliability. Those gates remain separate.

## Root Cause

The earlier composition described eight dynamic tasks: seven vendor tasks and
one incremental worker. The worker reserved one slot, then vendor bootstrap
incorrectly checked the complete eight-task total against the seven remaining
slots. After that double count was fixed, the real physical mismatch became
visible: seven 24 KiB vendor stacks require 172,032 bytes, while the diagnostic
fixture exposed only 98,304 bytes through its old shared-heap path.

The underlying architectural fault was that profile constants, composition,
linker storage, allocator state, and RTOS admission did not originate from one
resource capability. A correct arithmetic report therefore did not prove that
the final firmware could allocate the declared blocks.

## Implemented Contract

- `WifiResourcePlan` contains separate vendor and incremental-worker task
  groups; slot and stack totals are checked sums of those children.
- The current pinned archive inventory requires seven vendor tasks with 24 KiB
  stacks. The optional incremental worker requires one 8 KiB stack. Neither
  value is inferred by reducing a failing allocation.
- The RTOS allocates heterogeneous stack blocks outside its scheduler critical
  section, then publishes every group atomically in one critical section.
- Any dry-run, slot, or commit failure releases all previously allocated blocks
  and reservations. Group-aware errors include owner, required, available, and
  largest contiguous bytes.
- Resource-report schema v10 validates child sums and requires the final ELF
  `.hisi_shared_arenas` size to equal caller-owned arena storage plus runtime
  arena bytes.
- RF storage is claimed only after complete task admission succeeds, so an
  admission failure cannot leave partial RF initialization behind.

## Host And Build Evidence

The following checks passed before release:

- `hisi-alloc`: 7 host tests plus strict clippy;
- `hisi-rf-rtos-driver`: 12 host contract tests plus strict clippy;
- `hisi-rtos`: 75 host tests, compile-fail UI tests, and strict clippy;
- `hisi-rf-ws63`: 104 tests with the WPA2 incremental-worker feature set and
  strict clippy;
- independent locked package verification for every release unit using only
  crates.io dependencies.

Negative tests include a failure in the second task group after the first group
has allocated successfully. The final state has no stack block or reservation
from either group.

Two independently linked firmware profiles were checked against their v10
reports:

| Profile | Linked shared arena | Arena storage | Runtime arena | Dynamic tasks | Stack payload |
|---|---:|---:|---:|---:|---:|
| plain Cargo WPA2 | 303,168 B | 114,240 B | 188,928 B | 7 | 172,032 B |
| incremental scan | 299,072 B | 101,952 B | 197,120 B | 8 | 180,224 B |

The plain Cargo ELF also retained all 37 ROM patch entries and contained zero
vendor relocations.

## Silicon Evidence

The credential-free `incremental_scan_profile` was downloaded to two independent
WS63 boards at 3 MHz with full write verification. The same final ELF was used
for both boards; downloads completed in 90.34 and 82.59 seconds. After each
board's paired J-Link reset with UART capture already active, the image emitted:

- `RFDBG_A5B_BOOTSTRAP_OK`;
- `RFDBG_A5B_INITIALIZE_OK`;
- `RFDBG_A5B_SCAN_OK` with a non-empty scan result;
- `RFDBG_A5B_WORKER_RTOS_OK` with an 8 KiB worker stack;
- `RFDBG_A5B_SCAN_PROFILE_OK`.

The event queue reported zero drops on both boards. The second board returned
four scan results, completed both operations with zero backend errors, and
reported an 8 KiB worker stack, 3 ms maximum continuous run, and 1 ms maximum
ready latency. Neither board encountered a task-slot or task-stack admission
failure. These are true incremental-worker scans, not the older blocking
comparison path.

## Release Units

The verified dependency chain was released in order:

- `hisi-alloc 0.1.0-alpha.3`;
- `hisi-rf-rtos-driver 0.1.0-alpha.19`;
- `hisi-rf-core 0.1.0-alpha.21`;
- `hisi-rtos 0.1.0-alpha.19`;
- `hisi-rf-ws63 0.1.0-alpha.66`;
- `hisi-rf 0.1.0-alpha.76`.

Each upper release was packaged only after its exact lower dependencies were
available from crates.io.

## Remaining Boundary

HIL peak measurements calibrate headroom but do not redefine structural task
requirements. Pure-WPA3 resource calibration remains externally blocked until
a suitable SAE-only AP is available, and it does not block the now-closed
static admission contract.
