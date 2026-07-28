# WS63 A5U Operation Error Injection Evidence

## Scope

This evidence closes operation-level cancellation and backend-timeout
injection for the non-default WS63 incremental backend.

The credential-free fixture drives the production
`IncrementalBackendDriver` and private `IncrementalSupplicantBackend`. It does
not construct diagnostic errors directly:

- a running initialize operation receives a replacement command, transitions
  through `CancelRequested`, terminates as `Cancelled`, and releases the slot
  so the replacement operation can complete;
- a scan operation uses a one-millisecond deadline and an injected monotonic
  clock, reaches the production timeout path, and releases the slot so a new
  operation can start.

The fixture does not initialize RF hardware, consume AP credentials, prove
pure-WPA3 behavior, or prove every resource-lifecycle interleaving covered by
the wider A5 acceptance gate.

## Target Output

QEMU and real WS63 produced identical diagnostics:

```text
RFDBG_A5U_OPERATION_CANCEL_JSON {"schema":"hisi-rf-error/v2","code":"operation.cancelled","stage":"operation","action":"retry_operation","backend_code":0,"profile_revision":null,"trace":[],"trace_truncated":false,"docs":"errors-operation-cancelled"}
RFDBG_A5U_OPERATION_CANCEL_OK code=operation.cancelled stage=operation action=retry_operation
RFDBG_A5U_OPERATION_TIMEOUT_JSON {"schema":"hisi-rf-error/v2","code":"backend.timeout","stage":"scan","action":"retry_operation","backend_code":1462939652,"profile_revision":"ws63-wifi-2026-07-26","trace":[{"kind":"vendor_status","value":0}],"trace_truncated":false,"docs":"errors-backend-timeout"}
RFDBG_A5U_OPERATION_TIMEOUT_OK code=backend.timeout stage=scan action=retry_operation
```

The same final ELF ran in both environments. The hardware path retained full
probe-rs write verification at 3 MHz, completed the download in 2.38 seconds,
then used J-Link nRST and CH340 UART capture. The known flashboot
`Flash Init Fail! ret = 0x80001341` message remained outside the fixture's
success markers and is not classified as an RF failure.

## Release Evidence

- `hisi-rf-ws63 0.1.0-alpha.31` owns the production state-machine fixture and
  firmware example. Commit `68744a235ebf09ba4621cd52140f4d88fd0f1fdb`
  passed 85 host tests, clippy with warnings denied, standalone package
  verification, RV32 linking, QEMU, and real-silicon execution. Main CI run
  `30336483115` and publish run `30336721595` succeeded.
- `hisi-rf 0.1.0-alpha.41` pins that backend release. Commit
  `15dab16032ad3f29a09209f6d6d284b19bbd7490` passed package and dependency
  boundaries plus WPA2/WPA3 clean, offline, crates.io-only consumer builds on
  Linux, macOS, and Windows. Main CI run `30336949920` and publish run
  `30337275627` succeeded.

Operation-level cancellation, timeout classification, target serialization,
and terminal-slot recovery are closed. General owner, timer, queue, and key
resource conservation under all interleavings remains part of the separate A5
acceptance gate.
