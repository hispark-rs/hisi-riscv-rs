# WS63 A5U Typed-Error Target Parity Evidence

## Scope

This evidence covers target-side serialization and redaction parity for the
generic `operation.cancelled` and `backend.timeout` stable diagnostic classes.
It uses the same production `hisi-rf-error/v2` mapping on QEMU and real WS63.

It does not inject a real cancellation into an active radio operation, force a
WS63 backend timeout, or exercise association/EAPOL source classification.
Those operation-level and source-path cases remain separate gates.

## Target Fixture

The credential-free `typed_error_parity` firmware constructs the two generic
backend classes through the public production error mapping, serializes each
diagnostic into a fixed buffer, and writes the JSON and stable fields over
UART0. It does not initialize RF hardware, start the blob, scan, or consume AP
credentials.

QEMU and real WS63 produced the same output:

```text
RFDBG_A5U_CANCEL_JSON {"schema":"hisi-rf-error/v2","code":"operation.cancelled","stage":"operation","action":"retry_operation","backend_code":0,"profile_revision":null,"trace":[],"trace_truncated":false,"docs":"errors-operation-cancelled"}
RFDBG_A5U_CANCEL_OK code=operation.cancelled stage=operation action=retry_operation
RFDBG_A5U_BACKEND_TIMEOUT_JSON {"schema":"hisi-rf-error/v2","code":"backend.timeout","stage":"operation","action":"retry_operation","backend_code":7,"profile_revision":null,"trace":[],"trace_truncated":false,"docs":"errors-backend-timeout"}
RFDBG_A5U_BACKEND_TIMEOUT_OK code=backend.timeout stage=operation action=retry_operation
```

The hardware run used the same final ELF as QEMU. Probe-rs retained complete
write verification at 3 MHz and downloaded the planned image in 2.24 seconds
before J-Link nRST and CH340 UART capture.

## Release Evidence

- `hisi-rf-ws63 0.1.0-alpha.29` owns the target fixture. Its main CI run
  `30322046865` passed package, minimal target, four profile jobs, and stock
  rust-lld final links on Linux, macOS, and Windows. Publish run
  `30322230818` succeeded.
- `hisi-rf 0.1.0-alpha.39` pins that backend release. Its main CI run
  `30322310077` passed the WPA2/WPA3 external-consumer matrix on Linux, macOS,
  and Windows. Publish run `30322601660` succeeded.
- Local backend tests passed 66/66; standalone package verification and clippy
  passed before the release.

This closes cross-target JSON/UART/redaction parity for the two generic stable
classes. It does not close cancellation lifecycle correctness, a real backend
timeout recovery path, association rejection, or first-EAPOL timeout.
