# WS63 A5B Dual-Board Local-Neighbor Evidence

## Scope

This evidence covers a controlled two-board connectivity matrix. One WS63 ran
the vendor C SDK SoftAP as a temporary behavior oracle; the other ran the
public Rust `wifi_connectivity` image through the incremental backend. Network
credentials are intentionally absent from this record.

The same verified STA image was then booted through 20 J-Link nRST cycles while
the AP remained running. This is not Rust SoftAP evidence, a routed Internet
test, or a pure-WPA3 result.

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

## Repeated-Reset Matrix

The first scan attempt may finish with a backend timeout before its replacement
attempt succeeds. The example previously left that first attempt's
`WifiEvent::Failed` in the bounded event queue. The later success path then
observed the stale event instead of its own `ScanCompleted` event and halted.
The fix consumes and validates the event belonging to every completed scan
attempt before retrying or returning its result; the public scan marker remains
ordered after `RF3_SCAN_OK`.

After one full-verify download, the unchanged STA image completed 20 nRST runs:

- result: 20 pass, 0 fail, 0 marker-contract violation;
- authentication response-2 timeout: 0/20;
- DHCP, direct ARP reply, local-data-path marker, and lease renewal: 20/20;
- public DNS: skipped 20/20 with `reason=no-default-route`, as required for the
  isolated AP;
- event queue high-water: 1, drops: 0;
- runner errors: 0; maximum runner step: 35 ms;
- scan: 1,527--1,683 ms; connect: 234--241 ms;
- RTOS/RF allocation failures: 0/0; remaining measured headroom: 16,192 and
  48,232 bytes respectively.

The STA ELF SHA-256 is
`206908ac2a95b75ee4021cbe7be33ba7071387b56053b594288a5fc88455ab2c`.
The executable matrix summary SHA-256 is
`d1ae675c3478f510b968900a913ca061d3bd7af5d9c299fc2bf563170e78490b`.
Board roles were fixed for the matrix: J-Link `24060504` with UART
`/dev/cu.wchusbserial11110` ran the AP, while J-Link `23121310` with UART
`/dev/cu.wchusbserial11130` ran the STA. Device paths are evidence identifiers,
not portable build configuration.

## Separate Flash Observation

A later repeat did not produce additional RF evidence. One 3 MHz attempt failed
readback verification and the fallback could not attach. After physical reset,
another 3 MHz download passed full verification in 98.95 seconds, but the next
capture showed only the boot chain and no application marker. These events are
kept as probe/download/startup reliability observations and are not counted as
Wi-Fi failures or successes.

## Remaining Gates

A5B remains opt-in until its other release conditions are closed. This matrix
closes its repeated dual-board local connectivity parity item. A routed AP or
controlled LAN UDP/TCP echo service is still needed for a routed protocol-level
result, and a pure-WPA3 AP remains necessary for the SAE-only gate.
