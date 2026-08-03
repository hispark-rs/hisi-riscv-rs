# WS63 A5B Dual-Board Local-Neighbor Evidence

## Scope

This evidence covers one controlled two-board connectivity run. One WS63 ran
the vendor C SDK SoftAP as a temporary behavior oracle; the other ran the
public Rust `wifi_connectivity` image through the incremental backend. Network
credentials are intentionally absent from this record.

This is not Rust SoftAP evidence, a routed Internet test, a repeated-reset
reliability matrix, or a pure-WPA3 result.

## Fix Under Test

The incremental wait bridge previously represented each wake source with one
pending bit. A worker-response wake and a later EAPOL-input wake could coalesce
before the runner consumed readiness. The runner then collected the worker
response and waited forever for an edge that had already been lost.

The bridge now keeps a saturating pending count per source and consumes one
observation per poll. Backend input also exposes level readiness, so queued
EAPOL work remains visible even after an edge has been consumed. Host tests
cover two coalesced backend edges producing two ready polls.

## Verification

- The exact-feature host suite passed 109 tests.
- Strict Clippy and the RV32 target check passed.
- Plain Cargo final-link inspection found 37 ROM patch entries, zero vendor
  relocations, and the pinned upstream supplicant boundary.
- The planned binary was downloaded at 3 MHz with full readback verification.
- The captured ELF SHA-256 was
  `4d18cbd0430a4ef36d6c4492c0d4d671e5236c21cbb5e2764317f0f23d67e7f9`.

The STA emitted the following contract evidence:

- `W2D_WPA2_CONNECT_OK` and `A4_RADIO_EVENT kind=connected`;
- DHCP lease acquisition with no default router;
- one transmitted ARP request and one received ARP reply for the DHCP server;
- `RF5C_LOCAL_DATA_PATH_OK`;
- `RF5C_PUBLIC_DNS_SKIP reason=no-default-route` with zero DNS attempts and
  responses;
- `A4_NET_RUNNER_STEADY` and `A4_DHCP_RENEW_OK`;
- zero RF/RTOS allocation failures, queue drops, and runner errors.

After correcting the executable classifier so the route-disposition marker is
required but not incorrectly ordered against asynchronous ARP completion, the
preserved UART capture reclassified as one pass.

## Separate Flash Observation

A later repeat did not produce additional RF evidence. One 3 MHz attempt failed
readback verification and the fallback could not attach. After physical reset,
another 3 MHz download passed full verification in 98.95 seconds, but the next
capture showed only the boot chain and no application marker. These events are
kept as probe/download/startup reliability observations and are not counted as
Wi-Fi failures or successes.

## Remaining Gate

A5B remains opt-in. The next connectivity gate is an unchanged-image repeated
reset matrix using the two-board topology. A routed AP or controlled LAN
UDP/TCP echo service is still needed for a routed protocol-level result; the
isolated SoftAP run proves only the local L2/DHCP path.
