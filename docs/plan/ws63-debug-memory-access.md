# WS63 debug system-memory access diagnosis

## Status

Diagnostic execution completed on 2026-07-13. AP1 is a proven direct
system-memory path, while RISC-V SBA is not implemented. The AP1 path remains a
diagnostic result rather than a download default: probe-rs still needs an
explicit dual-AP transport contract before it can safely own AP0 for DMI and AP1
for system memory at the same time.

## D0: enumerate the DAP

- Enumerate every AP and record IDR, BASE, CFG, AP type and ROM table.
- Identify AHB-AP, AXI-AP and other MEM-AP instances separately from the AP0 DMI
  aperture.
- Compare the result with the WS63 vendor OpenOCD configuration. Evidence from
  other HiSilicon chips is not sufficient for WS63.
- Acceptance: a reproducible, read-only AP inventory with raw register values
  and decoded types.

### Evidence

An exhaustive APSEL `0..=255` scan found exactly two ADIv5 APs:

| AP | IDR | BASE | CFG | Observed CSW | Decoded role |
| --- | --- | --- | --- | --- | --- |
| AP0 | `0x44770002` | `0x80000003` | `0x00000000` | `0x80000042`, `0x80000052` | APB3 Memory-AP exposing the RISC-V DMI aperture at `0x80000000` |
| AP1 | `0x74770001` | `0x00000002` | `0x00000000` | `0x43800042`, `0x0b800052` | AHB3 Memory-AP with no CoreSight ROM table |

`BASE=0x00000002` on AP1 means that no ROM table is present; it does not mean
that AP1 memory transfers are unavailable. The vendor WS63 OpenOCD setup selects
AP0 with `-apsel 0 -dbgbase 0x80000000`, which agrees with AP0's DMI role but
does not enumerate AP1. CSW is recorded as an observation rather than immutable
identity: its transfer-size, increment and protection fields change as debug
operations configure the Memory-AP.

## D1: test address mappings safely

- Have the halted CPU write a sentinel into a reserved scratch SRAM range.
- Read only that range through each candidate AP, testing documented aliases or
  offsets before considering any write.
- Do not probe arbitrary peripheral, OTP, eFuse, secure or flash-control windows.
- If a mapping exists, record AP, offset, supported widths, access attributes,
  cache/coherency requirements and observed throughput.
- Acceptance: repeated sentinel reads agree with the CPU view before any direct
  AP write is attempted.

### Evidence

- With the hart halted, the CPU debug path wrote a 16-byte sentinel at scratch
  SRAM `0x00a70000`. AP1 returned the exact bytes at the same address; AP0
  returned zero. No AP-side offset or alias is required.
- AP1 returned matching little-endian data through 8-, 16- and 32-bit accesses.
- A controlled AP1 write/readback was visible through the CPU debug path, after
  which the original scratch contents were restored.
- The halt/write/read/restore/resume harness passed three times each for 4,096,
  65,532 and 65,536 bytes. The non-aligned 65,532-byte case covers a boundary
  that is not a 64 KiB transfer multiple.
- AP1 sustained approximately 85-93 KiB/s at a 2 MHz SWD clock. On an RF image
  that disables the flashboot watchdog, five 64 KiB reads and five 256 KiB reads
  all completed. Earlier failures while running `uart_hello` correlated with
  its inherited watchdog resetting the target during long reads, not with an
  AP1 address or width failure.

The experiment proves coherent access while the hart is halted. Cache and live
DMA coherency are not yet claimed; any production write path must halt the hart
or establish a stronger target-specific coherency contract.

## D2: verify RISC-V SBA

- Decode `sbcs`: `sbversion`, `sbasize`, `sbaccess32`, busy, error and
  autoincrement behavior.
- Treat the vendor OpenOCD setting `riscv set_prefer_sba off` as a risk signal.
- Only if the advertised capability is coherent, test read, then controlled
  write/readback in scratch SRAM, including busy/error recovery and throughput.
- Acceptance: repeated scratch tests and explicit error recovery; register
  presence alone is not evidence that SBA is usable.

### Evidence

AP0 exposed `SBCS=0x20040000`, decoded as:

- `sbversion = 1`;
- `sbasize = 0`;
- `sbaccess8/16/32/64/128 = false`.

The Debug Module therefore advertises no system-bus address width and no usable
access size. No `sbaddress*` or `sbdata*` write was attempted. This agrees with
the vendor setting `riscv set_prefer_sba off`: SBA is rejected as a WS63 memory
transport, rather than merely deprioritized.

## Capability rule

Any resulting fast path is an explicit target capability, disabled by default.
It requires read/write consistency, timeout and recovery tests, three repeated
downloads of both a small UART image and a large RF image, full verification,
and J-Link nRST boot markers before it can become the WS63 default.

The existing repeated-DMI DATA0 write optimization is a different path: it
accelerates the proven AP0 DMI aperture and does not claim direct system-memory
access. Its batch size and backpressure remain an explicit WS63 target
capability. Discovering another AP, an alias, or SBA must result in a separate
capability with its own fallback and evidence; it must not silently replace the
current transport.

For AP1, that future capability must describe at least the DMI AP, system-memory
AP, allowed RAM ranges, supported widths, maximum transfer size, halt/coherency
requirements, timeout and recovery behavior. The target configuration must opt
in; other `ArmWithRiscv` targets retain the current progbuf/DMI behavior. AP1
must not be hidden inside the WS63 flash algorithm or inferred solely from the
presence of a second Memory-AP.

## Remaining integration work

1. Give the `ArmWithRiscv` session a generic dual-AP ownership model instead of
   borrowing AP0 for the entire DMI transport lifetime.
2. Route only target-declared RAM ranges through AP1 and preserve progbuf/DMI as
   the fallback for registers, unsupported ranges and targets without the
   capability.
3. Add transport tests for missing capability, invalid ranges, target running,
   timeout and partial-transfer recovery.
4. Repeat full download, readback verification and J-Link nRST boot checks for a
   small UART image and a large RF image before enabling the capability for
   WS63.
