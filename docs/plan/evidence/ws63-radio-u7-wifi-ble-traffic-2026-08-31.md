# WS63 U7 Wi-Fi traffic + BLE advertising evidence (2026-08-31)

> **Status update (2026-09-01):** the open gates described below were later
> closed by the [connected BLE acceptance evidence](ws63-radio-u7-wifi-ble-connected-traffic-2026-09-01.md)
> and the corresponding connected SLE evidence. The original boundary is kept
> here as the conclusion supported by this earlier artifact.

## Scope

This evidence closes the U7 sub-gate for local Wi-Fi traffic while BLE
advertising and the shared radio runtime remain active. It does **not** prove
traffic during a BLE connection, Wi-Fi + SLE traffic, coexistence IRQ-latency
bounds, or stable public `coex` support.

The fixture uses two WS63 boards without an external access point:

- one board runs the repository WPA2 SoftAP and UDP echo service at
  `192.168.4.1:9`;
- one board advertises over BLE, performs three Wi-Fi scans, associates as a
  WPA2 STA, and recovers ten unique one-byte UDP echo sequences;
- both boards are reset for every matrix record while their ELF images remain
  unchanged.

## Implementation and executable contract

`hisi-rf-ws63` commit `f898f91` promotes the previous scan-only coexistence
fixture to a local-traffic fixture. The application keeps BLE advertising
active, verifies three scans, connects to the fixed peer, and uses a bounded
UDP sequence probe. Missing sequences are retried at most three times each;
the UART record includes unique sent/received counts, total attempts, and the
received bitmap. The probe therefore requires all ten unique responses while
making link-layer retry cost observable.

The parent HIL contract `ws63-wifi-ble-local-traffic/v1` fails closed on:

- missing initialization, BLE advertising, scan, association, or completion
  markers;
- any Wi-Fi/BLE event drop or firmware failure marker;
- fewer than three completed scans;
- fewer than ten unique STA responses or more than thirty attempts;
- fewer than ten SoftAP receives or replies.

Ten host tests cover the shared-init, activity, traffic, bounded-retry, failure,
and independent SoftAP counter paths. The repository Python/uv contract also
passes.

## Build and flash evidence

The release unit was tested outside the parent Cargo workspace to avoid parent
dependency overrides:

- host library tests: 142 passed;
- RV32 clippy with warnings denied: passed;
- RV32 release build and final resource/link checks: passed.

Fixed artifacts:

- STA + BLE ELF SHA-256:
  `83c0b3f8289d3925a9403957b84e86ffc1341d742007f6d4d60cdff131338294`;
- SoftAP ELF SHA-256:
  `1e6d9ddc2d45c7f0a9e682d337b104fc105734662c1ea46756b3d74e28507107`;
- transport: probe-rs at 3 MHz with full verify;
- measured download time: 151.29 seconds for the final STA + BLE image; the
  unchanged SoftAP image took 82.88 seconds.

## Silicon evidence

The original single-send fixture first passed 3/3, then produced a real
`19/20` counterexample. Run 17 completed all three scans, association, and BLE
advertising. The SoftAP received and replied to eleven UDP datagrams, but the
STA recovered only nine unique sequences. This was a data-probe reliability
failure, not a scan, association, BLE-event, RTOS-ownership, or SoftAP-service
failure. It must not be rewritten as a 20/20 result.

After adding bounded missing-sequence retry, the new fixed STA ELF passed 3/3
and then 20/20 paired nRST matrices without reflashing:

- 60/60 Wi-Fi scans completed while BLE advertising remained active;
- 20/20 WPA2 associations completed;
- 200/200 unique UDP echo sequences returned to the STA;
- total STA attempts were 226, with 11-13 attempts per reset
  (`11 x 15`, `12 x 4`, `13 x 1`);
- the SoftAP observed at least ten receives and replies in every reset;
- Wi-Fi and BLE event-drop counters remained zero;
- all 60 post-scan heap samples reported zero allocation failures and scan-clear
  progression `1 -> 2 -> 3`;
- RF arena peak was 46,036 bytes, with minimum observed free space 34,000 bytes.

Local matrix artifacts:

- initial counterexample:
  `/private/tmp/ws63-u7-wifi-ble-traffic-20reset-20260831`;
- final 3-reset:
  `/private/tmp/ws63-u7-wifi-ble-traffic-retry-3reset-20260831`;
- final 20-reset:
  `/private/tmp/ws63-u7-wifi-ble-traffic-retry-20reset-20260831`.

## Conclusion and remaining gate

The fixed artifacts demonstrate bounded, recoverable local Wi-Fi traffic while
BLE advertising is active, including reset reliability and measured retry/RAM
cost. U7 remains open for BLE-connected traffic, the corresponding Wi-Fi + SLE
lane, event/queue conservation under those workloads, and measured coexistence
latency before any stable `coex` promise.
