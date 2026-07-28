# A5UX Opaque Event Capacity Evidence

## Scope

This evidence closes the public API portion of A5UX event-capacity ownership.
It proves package, API, final-link, external-consumer, example, and template
contracts. It does not replace connectivity HIL and does not claim a new RF
behavior.

## Frozen Contract

- The public WS63 WPA2 and WPA3 profiles select eight bounded event slots at
  compile time.
- Ordinary facade signatures use `WifiController`, `RadioController`,
  `WifiParts`, and the corresponding incremental handles without an `EVENTS`
  const parameter.
- Applications declare `declare_radio_storage!(static RADIO_STORAGE)` without
  selecting queue capacity at the call site.
- `hisi-rf-resource-report/v6` remains the machine-readable source for
  `event_capacity` and its RAM cost.
- `hisi-rf-core` and `hisi-rf-ws63` retain generic-capacity implementation
  types and their queue-full, stale-generation, cancellation, and interleaving
  tests. A future public capacity change requires a named, calibrated profile.

## Release Evidence

- `hisi-rf 0.1.0-alpha.53` was published by workflow `30402253263`.
- CI run `30401878832` passed host tests, minimal `chip-ws63` package
  verification, RV32 WPA2/WPA3 checks, public API checks, and native
  Linux/macOS/Windows consumers.
- Follow-up run `30402601631` built the crates.io-only WPA2 and WPA3 fixture
  after replacing all public event-capacity generics with opaque facade types.
- The standalone release lock was generated outside the parent workspace and
  `cargo package --locked --features chip-ws63` verified the packaged tarball.

## Consumer Evidence

- WS63 examples commit `42fb055` builds `wifi_connectivity` in release mode for
  both WPA2 and WPA3 against the published alpha.53 facade.
- Template commit `94ec40c` generates the same no-argument storage declaration.
  A generated WS63 Wi-Fi project in a path containing spaces and non-ASCII
  characters completed a release build, emitted a v6 resource report, and
  produced a planned image.
- Template workflow `30403683603` passed the WS63/BS21 generation matrix and
  Linux/macOS/Windows resource-report jobs. GitHub prerelease
  `v0.7.0-alpha.14` publishes this contract.
- The generated report retained `event_capacity = 8`,
  `control_storage_bytes = 8544`, `radio_state_bytes = 2168`,
  `arena_storage_bytes = 303168`, and `caller_owned_bytes = 311712`.

No AP credential, credential-bearing firmware, or network secret is stored in
this evidence.
