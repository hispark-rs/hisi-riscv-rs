# WS63 Radio U5 Pairing And Bond Observation Evidence

## Scope

This evidence closes the fixed-image U5 pairing, authentication, and runtime
vendor-bond observation reliability gate. It does not prove persistence across
reset: all 40 final board boots reported an empty vendor table before pairing.

The tested ownership model is `VendorManaged + Rust observer`. The vendor host
retains its internal event-19 persistence callback. Rust observes a complete,
opaque 71-byte record only after successful authentication, validates its peer
address against the authenticated typed peer, emits a secret-free event, and
zeroizes the copied record. No key bytes enter public events, UART logs, or
`Debug` output.

## Immutable Inputs

- parent checkout: `940e36f6484f0a9290f1cac93e801ecf2a4aafeb`
- `hisi-rf-core`: `fde52ea27c29041a09593e59deb6d9701d3e8317`
- `hisi-rf`: `fb3f8071c0dab6cb8088aadadcf9588ef6743c1c`
- `hisi-rf-ws63`: `daeda31fe3f7199bc0427d4f79ff18112fa2e8f9`
- `ws63-radio-sys`: `0f10155968a997418a9de17d6731ccce30ae2201`
- `hisi-keystore`: `95a69a65bd0a026b145b0349e91a822adc07b6a1`
- peripheral ELF SHA-256:
  `76667b2a8c1ed6b8ba6abe1c8f3c5adcb3f14d356c0186f4f67416db6c9862d9`
- central ELF SHA-256:
  `94f1e08b0df627df3d48228d4f19cc0e2a73d206b9a0813aa04244fefd42a76e`
- peripheral FlashPlan image SHA-256:
  `faf276b990a712bd393a5b9000941d1608490afcec7f6e49d831706d13509454`
- central FlashPlan image SHA-256:
  `008c5f69b8157bd6b0cf3ef39aa60cbbb008997cb373b8be3c22ad200b7921b6`
- target: `riscv32imfc-unknown-none-elf`, release
- download: `hisi-fwpkg plan` image, probe-rs binary download at 3 MHz,
  complete verify

The final downloads completed in 58.93 seconds for the peripheral and 58.60
seconds for the central. The matrix then used only J-Link nRST and UART capture.

## Counterexamples And Fixes

The U5 path exposed three distinct integration defects before the final matrix:

1. ROM-mediated SMP calls reached missing ordered callbacks. The normalized BLE
   profile now roots the audited SMP and channel-map providers; final ELF symbol
   inspection proves strong `chnl_calc_ble_chnl_cls_to_chnl_map` and
   `chnl_calc_get_valid_map_num` providers remain linked.
2. Registering a second internal GAP event-19 callback replaced the vendor
   automatic persistence callback. The observer no longer registers that event;
   it reads the vendor-managed bounded table in runner context.
3. Polling that table before authentication raced the SMP write path, and using
   opaque byte 70 as a public address-type discriminator produced a false backend
   error. The final runner first drains lifecycle events, arms observation only
   after successful authentication, and binds the record to the authenticated
   typed peer after checking bytes 0..5.

Host facade tests passed 10/10 and host clippy passed with `-D warnings` before
the final build.

## Silicon Result

- shape gate: 3/3
- final paired nRST matrix: 20/20
- peripheral and central `AUTH_OK`: 40/40 board runs
- peripheral and central `BOND_OBSERVED`: 40/40 board runs
- peripheral and central `BOND_OK`: 40/40 board runs
- missing markers: 0
- failure markers, panic, missing callback, or event drop: 0

Every final run satisfied the secret-free observer conservation check:
`received = processed + dropped + pending`, with `dropped = 0` and
`pending = 0` at the pass marker.

Raw evidence remains under:

- `/private/tmp/ws63-u5-bond-peer-bound-3reset-20260812`
- `/private/tmp/ws63-u5-bond-peer-bound-20reset-20260812`

## Remaining Gate

Both boards reported `BOND_EMPTY` on every one of the 20 final resets;
`BOND_RESTORED` appeared zero times. This directly contradicts any claim that
vendor bond persistence or restore across nRST is complete. The next U5C work
must trace and close the vendor auto-save NVS path, then prove:

1. first boot empty, pair and save;
2. later reset restores the same peer without exposing secret bytes;
3. remove reports `NotPaired` and remains absent after another reset.

The 20/20 result is integration and statistical evidence for pairing,
authentication, observer ownership, and event conservation only. It is not a
formal proof and does not graduate the BLE API to stable.
