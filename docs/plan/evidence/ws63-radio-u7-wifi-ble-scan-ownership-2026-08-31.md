# WS63 U7 Wi-Fi + BLE scan ownership evidence (2026-08-31)

> **Status update (2026-09-01):** U7 was subsequently closed by the connected
> BLE/SLE traffic and schema-3 acceptance matrices. The original open-boundary
> statement remains below because it accurately describes this earlier fixture.

## Scope

This evidence closes a prerequisite defect in the U7 Wi-Fi + BLE activity
fixture: repeated Wi-Fi scans leaked vendor-owned event payloads after Rust
replaced the SDK scan callback. It does **not** close concurrent Wi-Fi traffic,
event-conservation, resource-peak, or RF coexistence acceptance.

## Root cause and fix

The vendor scan callback transfers ownership of the nested
`VendorScanResult.variable` allocation to its consumer. The original SDK
consumer frees that allocation and clears the pointer after processing it. The
Rust callback deep-copied the information elements but did not release the
transferred allocation, so repeated scans consumed the shared RF heap.

`hisi-rf-ws63` now uses an RAII guard for every callback payload with the same
ownership contract:

- scan result information elements;
- RX management frames;
- connection request and response information elements;
- disconnect information elements;
- external-auth SSID and PMKID, with PMKID zeroization before release.

The scan path also calls the vendor scan-result clear operation once after each
completed scan. A failed clear is surfaced as `wifi::Error::ClearScanResults`
instead of being silently ignored.

Fix commits:

- `e0facc6` - release transferred Wi-Fi event payloads;
- `8db7254` - keep scan diagnostics release-local;
- `2ec0eb5` and `3a22c93` - freeze the updated public error surface in all
  reviewed profiles.

## Build and host evidence

The release candidate was built against the published
`ws63-radio-sys 0.1.0-alpha.25`, without a parent-workspace path override.

- host library tests: 109 passed;
- RV32 release build: passed;
- RV32 clippy with warnings denied: passed;
- standalone `cargo package --locked`: passed;
- WPA2/WPA3 and BLE/SLE composition API snapshots: passed after intentional
  snapshot review.

Fixed activity ELF:

- path: `target/riscv32imfc-unknown-none-elf/release/wifi_ble_coexistence_smoke_8db7254`;
- SHA-256: `16fa0b50fbb4a64bbc92e4f314478900787985c35a1c8ef32adf468bf96e72fa`;
- flash transport: probe-rs at 3 MHz with full verify.

## Silicon evidence

The fixed ELF first completed three scans in one boot. It then ran a dual-board
three-reset matrix together with the unchanged SLE control image. All three
resets passed, for nine successful Wi-Fi scans in total.

Observed invariants in every reset:

- scan-clear count progressed `1 -> 2 -> 3`;
- allocation failures remained zero;
- post-first-scan live allocations stabilized at `0xe3` instead of growing per
  result;
- the activity image reached `RFDBG_COEX_WIFI_BLE_ACTIVITY_OK`;
- the peer completed shared SLE initialization.

Matrix artifacts are retained locally at
`/private/tmp/ws63-scan-owner-fix-3reset-20260831`.

## Conclusion and remaining gate

The repeated-scan heap growth is closed for the fixed image and ownership
contract. This result proves scan cleanup while BLE activity and shared radio
initialization are enabled. U7 remains open until a fixed two-board fixture
drives sustained local Wi-Fi traffic concurrently with BLE/SLE activity and
checks event conservation, resource peaks, reset reliability, and the first
failure boundary.
