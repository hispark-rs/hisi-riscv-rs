# WS63 BLE B2 Advertising And Scanning Evidence

## Scope

This evidence closes B2: two WS63 boards concurrently advertise and scan through
the bounded Rust event queue, with repeated real-silicon discovery. It does not
claim pairing, GATT, connection data transfer, coexistence, or a stable public BLE
API.

## Immutable Inputs

- `hisi-rf-ws63`: `2dda4108a230cbaef516a72df1e65f9923068bb5`
- `ws63-radio-sys`: `5a54cc41da8ff9bed017fedb404be2c92781d4fe`
- BLE profile revision: `ws63-ble-b2-discovery-closure-v40`
- release ELF SHA-256:
  `081aadf4357fd99a8f73386b4dd834cb694c624849effb98fa856a4488792867`
- build features: `ble-init,firmware-example`
- download path: `hisi-fwpkg` plan image followed by probe-rs binary download at
  3 MHz with complete verify

The callback profile stayed fail closed during bring-up. ROM return addresses
were mapped to coherent advertising, event-scheduler, coexistence, RX PHY, HCI,
low-power and DTS callback groups. Unknown callbacks were not globally enabled.

## Software Gates

- `cargo fmt --all -- --check`
- 18/18 `ws63-radio-sys` workspace host tests
- `cargo clippy --workspace --all-targets --target aarch64-apple-darwin -- -D warnings`
- RV32 release build of `ble_init_smoke`
- parent Python/uv contract check for the paired-board matrix script

## Silicon Result

Both boards ran the same ELF. Each reset started the peer first, then reset the
measured board so its scanner could observe the peer advertisement. The initial
three-run shape gate was 3/3. The committed matrix contract then completed 20/20:

```text
RFDBG_BLE_B1_INIT_OK
RFDBG_BLE_B2_COMMANDS_OK
RFDBG_BLE_B2_SCAN_READY
RFDBG_BLE_B2_ADV_OK
RFDBG_BLE_B2_SCAN_MATCH
```

All 20 measured and peer logs contained the four boot/operation markers. Every
measured log contained `RFDBG_BLE_B2_SCAN_MATCH`; the peer also observed the
measured board in all 20 runs. No run contained a missing ROM callback, BLE
command error, bounded-event drop, or event decode error.

The executable evidence contract is
`hil/ws63-ble-discovery-reset-matrix.py`. Raw logs and `summary.json` for this
run were captured under
`/private/tmp/ws63-ble-b2-v40-reset20-20260807` on the HIL host.

## Boundary

This is integration and statistical regression evidence for one fixed image,
two boards, and the stated environment. It demonstrates B2 behavior and checks
the callback/event ownership path. It is not a mathematical RTOS proof and does
not prove that RF traffic or external environments can never lose events.

