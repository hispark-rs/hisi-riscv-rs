# WS63 U7 Wi-Fi traffic + connected BLE evidence (2026-09-01)

## Scope

This evidence closes the U7 BLE-connected traffic sub-gate. Two WS63 boards
maintain a BLE connection while the central also scans for Wi-Fi, associates
with the peer's WPA2 SoftAP, and exchanges local UDP traffic. It does **not**
establish a bounded coexistence IRQ-latency contract, calibrate the final shared
arena watermark, or make the public `coex` API stable.

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

## Fixed artifacts

- SoftAP + BLE peripheral ELF SHA-256:
  `084203f1c588863f3109fb6b83c94e2ff8b4cae23967053e084c39bf569d2c21`;
- STA + BLE central ELF SHA-256:
  `7d27710b77193479f3632b01d89cdfbaa3b1754244ec531a127ac2e1fec081ab`;
- both images were downloaded through probe-rs at 3 MHz with full readback
  verification on the first attempt;
- the SoftAP download took 131.82 seconds and the central download took 147.86
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

Local matrix artifacts:

- diagnostic failure: `/private/tmp/ws63-wifi-ble-connected-u7-20260901/reset3`;
- corrected shape gate: `/private/tmp/ws63-wifi-ble-connected-u7-20260901/reset3-v2`;
- corrected acceptance matrix:
  `/private/tmp/ws63-wifi-ble-connected-u7-20260901/reset20-v2`.

## Remaining boundary

U7 remains open for explicit event-conservation accounting and measured
IRQ/resource latency and watermark acceptance. This matrix is integration and
statistical evidence for two fixed artifacts; it is not a proof that external
RF traffic cannot be lost and does not graduate the public coexistence API.
