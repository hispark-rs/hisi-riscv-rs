# WS63 U7 Wi-Fi traffic + connected BLE evidence (2026-09-01)

## Scope

This evidence closes the U7 BLE-connected traffic and acceptance gates. Two WS63 boards
maintain a BLE connection while the central also scans for Wi-Fi, associates
with the peer's WPA2 SoftAP, and exchanges local UDP traffic. Each generation
also checks bounded event conservation, RF-heap watermarks, scheduler ownership,
ready latency, and IRQ span. This does **not** make the public `coex` API stable.

## Implementation and contract

`hisi-rf-ws63 0.1.0-alpha.97` (commit `0900d05`) provides the connected BLE
central fixture and the SoftAP plus BLE composition. `hisi-rf
0.1.0-alpha.106` (commit `27962ad`) pins that backend. The SoftAP fixture was
completed in the WS63 examples commits `c7636de` and `bc21c19`; the latter
classifies successful advertising-data and advertising-parameter completions as
success instead of treating status zero as an error.

The parent HIL contract `ws63-wifi-ble-connected-local-traffic/v1`, committed
in `05dae2696`, requires in every reset generation:

- BLE advertising readiness and a central/peripheral connection marker on both
  roles;
- three completed Wi-Fi scans, WPA2 association, and ten unique local UDP echo
  responses;
- no Wi-Fi/BLE event-drop or firmware failure marker;
- parseable SoftAP echo counters with at least ten receives and replies;
- the final client marker proving that BLE remains connected when Wi-Fi traffic
  completes.

The schema-3 acceptance extension is implemented by `hisi-rf-core` commit
`b5db5e2`, `hisi-rf-ws63` commits `420b421`, `17d786d`, and `a8005de`, the
WS63 examples commit `fd15c7b`, and parent commits `cc490d9d4` and `7f4c6c017`.
It additionally requires `accepted = consumed + pending`, zero event drops and
allocation failures, no RTOS ready-ownership error, client RF-heap free space
of at least 8 KiB, SoftAP RF-heap free space of at least 16 KiB, ready latency
at most 2 seconds, and IRQ span at most 100 ms. The RF-heap gate is deliberately
separate from the profile's 16 KiB RTOS runtime-object headroom.

## Fixed artifacts

- SoftAP + BLE peripheral ELF SHA-256:
  `c7543047ecc409abf267370a0f5fae57de5818185b09178d31e8ef7e6b0390f8`;
- STA + BLE central ELF SHA-256:
  `8a667551011499f1208d6f9614de742b200335835164e25d9663250c3e77a1b9`;
- both images were downloaded through probe-rs at 3 MHz with full readback
  verification on the first attempt;
- the SoftAP download took 132.60 seconds and the central download took 132.28
  seconds;
- the reset matrices reused the unchanged images and used nRST only.

## Silicon evidence

The first 0/3 diagnostic matrix failed before BLE advertising because the
SoftAP event handler misclassified successful status-zero completions and
panicked. After the narrow event classification fix, the fixed images completed
a 3/3 shape gate and a fresh 20/20 paired nRST matrix:

- 20/20 BLE links reached the connected marker on both roles;
- 60/60 Wi-Fi scans completed;
- 20/20 WPA2 associations completed;
- 200/200 unique UDP echo sequences returned to the STA, exactly 10/10 in each
  reset and with zero zero-reply runs;
- the SoftAP counters recorded 220 receives and 220 replies;
- no contract failure marker or missing required marker was observed.

The acceptance images then completed another 3/3 shape gate and 20/20 paired
nRST matrix. Across the 20-run matrix:

- all 40 role snapshots satisfied event conservation and recorded zero drops;
- the largest Wi-Fi/BLE queue high-water mark was 8;
- the minimum RF-heap free space was 10,364 bytes and the maximum peak usage was
  63,780 bytes, with zero allocation failures;
- the maximum observed ready latency was 1,189 ms and maximum IRQ span was 1 ms;
- all ready-owner, duplicate-membership, wrong-bucket, and invalid-link counters
  remained zero;
- the connected-traffic result remained 20/20 and 200/200 unique UDP sequences.

Local matrix artifacts:

- diagnostic failure: `/private/tmp/ws63-wifi-ble-connected-u7-20260901/reset3`;
- corrected shape gate: `/private/tmp/ws63-wifi-ble-connected-u7-20260901/reset3-v2`;
- corrected acceptance matrix:
  `/private/tmp/ws63-wifi-ble-connected-u7-20260901/reset20-v2`.
- acceptance shape gate:
  `/private/tmp/ws63-u7-acceptance-ble-rf8k-reset3-20260901`;
- acceptance matrix:
  `/private/tmp/ws63-u7-acceptance-ble-rf8k-reset20-20260901`.

## Remaining boundary

The BLE half of U7 event-conservation and resource/latency acceptance is closed.
This matrix is integration and statistical evidence for two fixed artifacts;
it is not a mathematical proof, cannot prove that external RF traffic never
loses data, and does not graduate the public coexistence API. Stable graduation
is a separate U8 review.
