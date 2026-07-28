# WS63 A5U Operation Error Injection Evidence

## Scope

This evidence closes public controller-path cancellation and backend-timeout
injection for the non-default WS63 incremental backend.

The credential-free fixture drives the production
`WifiController`, facade command/completion/event channels,
`IncrementalRadioRunner`, and private `IncrementalSupplicantBackend`. It does
not construct diagnostic errors or call the backend driver directly:

- a public connect future is dropped, a public disconnect request replaces it,
  the runner transitions through `CancelRequested` and `Cancelled`, the
  replacement completes, and the cancellation diagnostic is read back through
  `WifiController::next_event`;
- a second public connect future uses a one-millisecond deadline and an injected
  monotonic clock, reaches the production connect-timeout path, and receives
  the backend error through the completion channel.

The fixture does not initialize RF hardware, consume AP credentials, prove
pure-WPA3 behavior, or prove every resource-lifecycle interleaving covered by
the wider A5 acceptance gate.

## Target Output

QEMU and real WS63 produced identical diagnostics:

```text
RFDBG_A5U_OPERATION_CANCEL_JSON {"schema":"hisi-rf-error/v2","code":"operation.cancelled","stage":"operation","action":"retry_operation","backend_code":0,"profile_revision":null,"trace":[],"trace_truncated":false,"docs":"errors-operation-cancelled"}
RFDBG_A5U_OPERATION_CANCEL_OK code=operation.cancelled stage=operation action=retry_operation
RFDBG_A5U_OPERATION_TIMEOUT_JSON {"schema":"hisi-rf-error/v2","code":"backend.timeout","stage":"connect","action":"retry_operation","backend_code":1462939652,"profile_revision":"ws63-wifi-2026-07-26","trace":[{"kind":"vendor_status","value":0}],"trace_truncated":false,"docs":"errors-backend-timeout"}
RFDBG_A5U_OPERATION_TIMEOUT_OK code=backend.timeout stage=connect action=retry_operation
```

The same final ELF ran in both environments. The hardware path retained full
probe-rs write verification at 3 MHz, completed the download in 2.49 seconds,
then used J-Link nRST and CH340 UART capture. The known flashboot
`Flash Init Fail! ret = 0x80001341` message remained outside the fixture's
success markers and is not classified as an RF failure.

## Release Evidence

- `hisi-rf-ws63 0.1.0-alpha.36` owns the public-source-path fixture and firmware
  example. Commit `407ae8a4f40914d0c23a814f44fefcc84684102d` passed 89 host
  tests, clippy with warnings denied, standalone package verification, RV32
  linking, QEMU, and real-silicon execution. Main CI run `30357162605` and
  publish run `30357192915` succeeded; CI also passed all four security/backend
  profiles and Linux/macOS/Windows final RF links.
- `hisi-rf 0.1.0-alpha.46` pins that backend release. Commit
  `868537be983843fea1b64b4afd75901e0c92c0da` passed local standalone package,
  facade host/clippy and RV32 incremental composition checks. Publish run
  `30357480933` succeeded. Follow-up commit `c633455145319bfa3c52cbca0b3d92682f136d9d`
  advanced the crates.io-only consumer fixture to the released facade and
  restored the changelog comparison links; main CI run `30357989507` passed
  the API/dependency boundaries plus WPA2/WPA3 clean, offline and final-link
  consumer builds on Linux, macOS and Windows.

Public controller-path cancellation, timeout classification, target
serialization, and terminal-slot recovery are closed. The complete
init/scan/connect/DHCP/renew artifact parser and released-state-machine response
bound remain separate A5 acceptance gates.
