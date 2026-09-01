# WS63 U7 Wi-Fi traffic + SLE announce evidence (2026-09-01)

## Scope

This evidence closes the U7 sub-gate for local Wi-Fi traffic while SLE
announcing and the shared radio runtime remain active. It does **not** prove
traffic during an SLE connection, BLE-connected traffic, coexistence
IRQ-latency bounds, or stable public `coex` support.

The fixture uses two WS63 boards without an external access point:

- one board runs the repository WPA2 SoftAP and UDP echo service at
  `192.168.4.1:9`;
- one board announces over SLE, performs three Wi-Fi scans, associates as a
  WPA2 STA, and recovers ten unique one-byte UDP echo sequences;
- both boards are reset for every matrix record while their ELF images remain
  unchanged.

## Implementation and executable contract

`hisi-rf-ws63` commit `ef51861` extends the shared coexistence fixture with an
SLE announce task and reuses the bounded Wi-Fi scan/connect/local-echo path.
The activity board waits for a successful `AnnounceEnabled` event before
starting Wi-Fi work, keeps the announce task alive throughout the probe, and
fails closed on SLE event drops or announce errors.

The SLE closure leaves 4,032 fewer bytes than the BLE closure before the fixed
task stacks after the network executor is linked. The profile therefore records
separate measured shared-arena limits: 272 KiB for Wi-Fi+SLE and 276 KiB for
Wi-Fi+BLE. The final linker overlap guard remains authoritative; no stack was
reduced and no guard was bypassed.

The parent HIL contract `ws63-wifi-sle-local-traffic/v1` fails closed on:

- missing shared initialization, SLE announce, scan, association, local echo,
  or final completion markers;
- any Wi-Fi/SLE event drop or firmware failure marker;
- fewer than three completed scans;
- fewer than ten unique STA responses or more than thirty attempts;
- fewer than ten SoftAP receives or replies.

Thirteen host tests cover the shared-init, BLE traffic, SLE traffic, endpoint
role mapping, bounded echo, and failure paths. The repository Python/uv
contract passes.

## Build and flash evidence

The release unit was checked in an independent local clone so parent workspace
dependency overrides could not hide standalone failures:

- SLE-feature host library tests: 137 passed;
- Wi-Fi+SLE RV32 final release link and resource guards: passed;
- Wi-Fi+BLE RV32 final release link regression: passed;
- `cargo fmt --all -- --check` and `git diff --check`: passed.

Fixed artifacts:

- STA + SLE ELF SHA-256:
  `e844182631bf1005806ff4567f2b16d58d5a4420febf4624f8ff7b9018c60549`;
- SoftAP ELF SHA-256:
  `1e6d9ddc2d45c7f0a9e682d337b104fc105734662c1ea46756b3d74e28507107`;
- activity-board transport: probe-rs at 3 MHz with full verify;
- measured activity-image download time: 164.47 seconds.

## Silicon evidence

The fixed images first passed a 3/3 shape gate and then a fresh 20/20 paired
nRST matrix without reflashing:

- 60/60 Wi-Fi scans completed while SLE announcing remained active;
- 20/20 WPA2 associations completed;
- 200/200 unique UDP echo sequences returned to the STA;
- total STA attempts were 220, exactly 11 attempts in every reset, so the
  bounded retry cost remains visible rather than being reported as zero loss;
- the SoftAP independently recorded 220 receives and 220 replies, with at
  least 11 of each in every reset;
- Wi-Fi and SLE event-drop counters remained zero and no failure marker was
  observed;
- all 60 post-scan heap samples reported zero allocation failures and scan
  cleanup progression `1 -> 2 -> 3`;
- the RF heap arena was 71,104 bytes, maximum observed peak was 44,136 bytes,
  and minimum observed free space was 32,384 bytes.

Local matrix artifacts:

- shape gate:
  `/private/tmp/ws63-u7-wifi-sle-traffic-3reset-20260831`;
- final matrix:
  `/private/tmp/ws63-u7-wifi-sle-traffic-20reset-20260901`.

## Conclusion and remaining gate

The fixed artifacts demonstrate bounded, recoverable local Wi-Fi traffic while
SLE announcing is active, including reset reliability and measured retry/RAM
cost. U7 remains open for BLE-connected traffic, SLE-connected traffic, and
measured coexistence IRQ/resource latency before any stable `coex` promise.
