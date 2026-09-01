# WS63 U7 Wi-Fi traffic + connected SLE evidence (2026-09-01)

## Scope

This evidence closes the U7 SLE-connected traffic and acceptance gates. Two WS63 boards
maintain an SLE connection while the client performs Wi-Fi scans, associates
with the peer's WPA2 SoftAP, and exchanges local UDP traffic. Each generation
also checks bounded event conservation, RF-heap watermarks, scheduler ownership,
ready latency, and IRQ span. It does **not** make the public `coex` API stable.

## Implementation and contract

The client fixture comes from `hisi-rf-ws63` commit `227b931`. It retains a
previously observed coexistence AP candidate across later scan rounds instead
of clearing the only usable target. The peer SoftAP comes from the WS63 examples
commit `2991513`, where RTOS scheduler storage and the RF shared arena have
separate ownership.

The parent HIL contract `ws63-wifi-sle-connected-local-traffic/v1` requires, in
every reset generation:

- SLE server readiness and a client/server SLE connection;
- three completed Wi-Fi scans and retention of a usable SoftAP candidate;
- WPA2 association and ten unique local UDP echo responses;
- zero Wi-Fi/SLE event drops and no firmware failure marker;
- at least ten SoftAP echo receives and replies.

The HIL parser and its host tests were committed in parent commit `82895d488`.
The schema-3 acceptance extension is implemented by `hisi-rf-core` commit
`b5db5e2`, `hisi-rf-ws63` commits `420b421`, `17d786d`, and `a8005de`, the
WS63 examples commit `fd15c7b`, and parent commits `cc490d9d4` and `7f4c6c017`.
It requires `accepted = consumed + pending`, zero event drops and allocation
failures, no RTOS ready-ownership error, client RF-heap free space of at least
8 KiB, SoftAP RF-heap free space of at least 16 KiB, ready latency at most 2
seconds, and IRQ span at most 100 ms.

## Fixed artifacts

- SoftAP + SLE server ELF SHA-256:
  `df30af903b4bd8193c14b5409a8f9e102786d93a65f9a680f2c7b708c4f07f34`;
- STA + SLE client ELF SHA-256:
  `41461b7b14b9a8a1f07d987c0196ff82b49ea7aa3573343cb78d136374eb3657`;
- both images were downloaded through probe-rs at 3 MHz with full verify;
- the SoftAP download took 139.58 seconds and the client download took 148.82
  seconds;
- the 20-reset matrix reused the unchanged images and used nRST only.

## Silicon evidence

After a 3/3 shape gate, the fixed images completed a fresh 20/20 paired nRST
matrix:

- 20/20 SLE connections reached the connected marker on both roles;
- 60/60 Wi-Fi scans completed;
- 20/20 WPA2 associations completed;
- 200/200 unique UDP echo sequences returned to the STA;
- the STA used 228 bounded attempts in total, 10 to 13 per reset;
- the SoftAP recorded 225 receives and 225 replies, at least 10 per reset;
- Wi-Fi and SLE event-drop counters remained zero;
- every client completion marker reported three scans, ten echoes, and active
  SLE connectivity.

The acceptance images then completed another 3/3 shape gate and 20/20 paired
nRST matrix. Across the 20-run matrix:

- all 40 role snapshots satisfied event conservation and recorded zero drops;
- the largest Wi-Fi/SLE queue high-water mark was 5;
- the minimum RF-heap free space was 13,336 bytes and the maximum peak usage was
  61,876 bytes, with zero allocation failures;
- the maximum observed ready latency was 1,212 ms and maximum IRQ span was 1 ms;
- all ready-owner, duplicate-membership, wrong-bucket, and invalid-link counters
  remained zero;
- the connected-traffic result remained 20/20 and 200/200 unique UDP sequences.

Local matrix artifact:

`/private/tmp/ws63-u7-wifi-sle-connected-retained-scan-20reset-20260901`

Acceptance artifacts:

- `/private/tmp/ws63-u7-acceptance-sle-rf8k-reset3-20260901`;
- `/private/tmp/ws63-u7-acceptance-sle-rf8k-reset20-20260901`.

## Release and remaining boundary

The retained-candidate fix and the 272 KiB Wi-Fi+BLE final-link correction are
released in `hisi-rf-ws63 0.1.0-alpha.94`; the facade pins that backend in
`hisi-rf 0.1.0-alpha.104`. Backend CI builds the final radio ELF on Linux,
macOS, and Windows. The 272 KiB Wi-Fi+BLE figure is a link-time boundary, not a
new BLE runtime watermark claim.

The SLE half of U7 event-conservation and resource/latency acceptance is closed.
Together with the BLE acceptance matrix, this closes U7 integration acceptance.
The result is integration and statistical evidence for fixed artifacts, not a
mathematical proof that external RF traffic cannot be lost. The later U8 review
recorded a no-go graduation decision in
[`hisi-rf-u8-stable-graduation-review-2026-09-01.md`](hisi-rf-u8-stable-graduation-review-2026-09-01.md).
