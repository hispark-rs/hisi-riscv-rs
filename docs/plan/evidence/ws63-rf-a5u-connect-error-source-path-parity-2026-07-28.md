# WS63 A5U Connect-Error Source-Path Parity Evidence

## Scope

This evidence covers target-side classification and serialization parity for
two production WS63 connect-error builders:

- IEEE 802.11 association status 30, classified as a PMF-stage temporary
  rejection;
- association success followed by expiry before the first EAPOL frame,
  classified as an EAPOL-stage backend timeout.

The credential-free fixture calls the same private builders as the production
WS63 connect loop. It does not duplicate the status/stage mapping in example
code, initialize RF hardware, start the radio blob, scan, or consume AP
credentials.

This proves that the production source evidence reaches the stable
`hisi-rf-error/v2` schema unchanged on RV32, QEMU, and real WS63. It does not
force a live AP to reject association or drop the first EAPOL frame, and it does
not replace the existing real-connect recovery evidence.

## Target Output

QEMU and real WS63 produced identical source-path diagnostics:

```text
RFDBG_A5U_ASSOC_REJECTION_JSON {"schema":"hisi-rf-error/v2","code":"wifi.connection_failed","stage":"pmf","action":"inspect_network_and_retry","backend_code":30,"profile_revision":"ws63-wifi-2026-07-26","trace":[{"kind":"ieee_status","value":30},{"kind":"supplicant_context","value":1093},{"kind":"driver_context","value":291}],"trace_truncated":false,"docs":"errors-wifi-connection-failed"}
RFDBG_A5U_ASSOC_REJECTION_OK code=wifi.connection_failed stage=pmf action=inspect_network_and_retry
RFDBG_A5U_FIRST_EAPOL_TIMEOUT_JSON {"schema":"hisi-rf-error/v2","code":"backend.timeout","stage":"eapol","action":"retry_operation","backend_code":3022192725,"profile_revision":"ws63-wifi-2026-07-26","trace":[{"kind":"supplicant_context","value":1059},{"kind":"driver_context","value":85},{"kind":"vendor_status","value":0},{"kind":"ieee_status","value":0}],"trace_truncated":false,"docs":"errors-backend-timeout"}
RFDBG_A5U_FIRST_EAPOL_TIMEOUT_OK code=backend.timeout stage=eapol action=retry_operation
```

The same final ELF ran in both environments. The hardware path retained full
probe-rs write verification at 3 MHz and completed the download in 2.65
seconds, followed by J-Link nRST and CH340 UART capture. The first capture
attempt used a stale serial-device suffix after the download; re-enumerating
the CH340 and running only nRST plus capture produced the output above without
rewriting flash.

## Release Evidence

- `hisi-rf-ws63 0.1.0-alpha.30` owns the hidden firmware-only fixture and the
  production builders. Main CI run `30334902194` passed the standalone package,
  minimal target, WPA2/WPA3 blocking and incremental profiles, and stock
  rust-lld final links on Linux, macOS, and Windows. Publish run
  `30335131492` succeeded.
- `hisi-rf 0.1.0-alpha.40` pins the backend release. Main CI run
  `30335307515` passed the WPA2/WPA3 crates.io consumer matrix on Linux,
  macOS, and Windows. Publish run `30335653352` succeeded.
- Local backend validation passed 66 host tests, clippy with warnings denied,
  standalone package verification, the RV32 release link, QEMU execution, and
  real-silicon UART capture.

Association-rejection and first-EAPOL source-path target parity are closed.
Operation-level cancellation and backend-timeout injection remain separate
A5U work.
