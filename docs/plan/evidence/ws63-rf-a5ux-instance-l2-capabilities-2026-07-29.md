# A5UX instance-owned L2 capability evidence

Status: host, release, and consumer closure complete; final transition-profile
HIL pending.

## Contract

Station identity is immutable capability data published by a successfully
initialized radio instance:

- `WifiL2Capabilities` validates that the station address is neither all-zero
  nor multicast.
- Each `RadioState` owns its own capability state. Failed initialization does
  not publish partial identity, and a later successful initialization publishes
  atomically.
- `WifiDevice::l2_capabilities()` and
  `WifiDevice::station_mac_address()` are the safe application accessors.
- The WS63 netif accessor is crate-private, and the public facade no longer
  exports a process-global `station_mac_address()` function.

This is deliberately not application configuration. Credentials, operation
timeouts, backend timeouts, application deadlines, and scan capacity remain in
the example/template configuration module; the station MAC only exists after
the selected backend initializes its concrete device.

## Released units

| Unit | Commit | Release | Publish workflow |
| --- | --- | --- | --- |
| `hisi-rf-core` | `d027189` | `v0.1.0-alpha.18` | `30411760350` |
| `hisi-rf-ws63` | `934aa9b` | `v0.1.0-alpha.45` | `30412557706` |
| `hisi-rf` | `8bb5839` | `v0.1.0-alpha.55` | `30413863048` |

The WS63 examples consume facade alpha.55 at commit `2893ea8`. The generated
Wi-Fi starter consumes the same facade at template commit `0fa35a3` and tag
`v0.7.0-alpha.16`.

## Verification

- `hisi-rf-core`: 18 default and 52 all-feature host tests, including
  unavailable-before-init, failed-init recovery, blocking/incremental
  publication, and two independent radio identities; clippy, RV32 check,
  public-API snapshot, and standalone package also passed.
- `hisi-rf-ws63`: WPA2 and WPA3 host matrices, clippy, RV32 checks, public-API
  snapshots, standalone package, and final-link CI passed. Resource truth was
  updated to 8608 bytes of control storage and a 2216-byte eight-event
  `RadioState`.
- `hisi-rf`: host tests, clippy, RV32 checks, WPA2/WPA3 facade API parity,
  standalone package, and macOS/Linux/Windows clean and offline external
  consumer CI passed in run
  [30413596846](https://github.com/hispark-rs/hisi-rf/actions/runs/30413596846).
  The crates.io-only fixture was then advanced to alpha.55.
- `wifi_connectivity`: WPA2 and WPA3 RV32 release builds passed with the
  smoltcp runner obtaining the MAC from its own `WifiDevice`.
- generated WS63 Wi-Fi project: release build and resource-report generation
  passed locally; main CI
  [30414286905](https://github.com/hispark-rs/hisi-rs-template/actions/runs/30414286905)
  passed on macOS, Linux, and Windows.
- the parent workspace passed its locked RV32 workspace check with
  `hisi-rf/chip-ws63`.

These checks establish ownership, API, packaging, and cross-platform consumer
contracts. They do not replace the final transition-profile real-silicon
matrix. Pure WPA3 remains an external blocked gate because the available AP
does not expose a pure WPA3 mode.
