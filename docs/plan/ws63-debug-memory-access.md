# WS63 debug system-memory access diagnosis

## Status

Deferred, read-only-first diagnostic work. It must not interrupt connectivity
HIL or change a board while a download benchmark is in progress.

The current evidence only rules out using AP0 with unmodified CPU addresses:
reads from CPU SRAM `0x00a00000` through that path returned zero. It does not
prove that WS63 lacks another system-memory AP, an AP-side address alias, or a
usable RISC-V Debug Module SBA implementation.

## D0: enumerate the DAP

- Enumerate every AP and record IDR, BASE, CFG, AP type and ROM table.
- Identify AHB-AP, AXI-AP and other MEM-AP instances separately from the AP0 DMI
  aperture.
- Compare the result with the WS63 vendor OpenOCD configuration. Evidence from
  other HiSilicon chips is not sufficient for WS63.
- Acceptance: a reproducible, read-only AP inventory with raw register values
  and decoded types.

## D1: test address mappings safely

- Have the halted CPU write a sentinel into a reserved scratch SRAM range.
- Read only that range through each candidate AP, testing documented aliases or
  offsets before considering any write.
- Do not probe arbitrary peripheral, OTP, eFuse, secure or flash-control windows.
- If a mapping exists, record AP, offset, supported widths, access attributes,
  cache/coherency requirements and observed throughput.
- Acceptance: repeated sentinel reads agree with the CPU view before any direct
  AP write is attempted.

## D2: verify RISC-V SBA

- Decode `sbcs`: `sbversion`, `sbasize`, `sbaccess32`, busy, error and
  autoincrement behavior.
- Treat the vendor OpenOCD setting `riscv set_prefer_sba off` as a risk signal.
- Only if the advertised capability is coherent, test read, then controlled
  write/readback in scratch SRAM, including busy/error recovery and throughput.
- Acceptance: repeated scratch tests and explicit error recovery; register
  presence alone is not evidence that SBA is usable.

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
