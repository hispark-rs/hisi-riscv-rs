# WS63 SLE S2 Connect And Disconnect Evidence

## Scope

This evidence closes S2: two WS63 boards complete SLE announce, seek, connect,
and client-initiated disconnect through one controller-owned bounded event
queue. It does not claim SSAP data transfer, pairing UX, coexistence, or a
stable public SLE API.

## Immutable Inputs

- `ws63-radio-sys`: `9d35735e7182e5f8629d8d3b6b5083e459e6d38b`
- `hisi-rf-ws63`: `565307f51412c7279a89a83598880b85a1dc944f`
- SLE archive profile: `ws63-sle-s0-archive-abi-v1`
- SLE init profile: `ws63-sle-s2-connect-v1`
- server ELF SHA-256:
  `7bb1e394eb925476eaa318559cb27a547bf5db145c42fa7206bc089249e45fa8`
- client ELF SHA-256:
  `684d73853619032364d453bfa4ae5067ddf9604e4d65d9aa6b67ee029e489fe3`
- server FlashPlan image SHA-256:
  `929de8533de9daedd298de803dc7b956d0e64ac348f96552d87c1e8b2d8a16b3`
- client FlashPlan image SHA-256:
  `9a46c79e44005924ae966018b17669d58033500b201d6725afc59df2e82a1240`
- build features: `sle-init,firmware-example`
- download path: `hisi-fwpkg plan` image followed by probe-rs binary download
  at 3 MHz with complete verify; each image completed in about 67 seconds

The raw ABI is limited to local-address setup, default connection parameters,
connection callback registration, connect, and disconnect. Only the connection
state callback is installed; all other callback slots remain null. The callback
copies the peer address and scalar state into the existing bounded queue before
returning to vendor code.

## Software Gates

- `cargo fmt --all -- --check`
- 23/23 `hisi-rf-ws63` host library tests
- host clippy for the SLE feature with `-D warnings`
- RV32 release links and clippy for `sle_connect_server_smoke` and
  `sle_connect_client_smoke`
- `ws63-radio-sys` workspace host tests/clippy and RV32 SLE check
- 3/3 unit tests for `hil/ws63-sle-connect-reset-matrix.py`
- parent Python uv contract check
- successful GitHub CI for both child commits

## Silicon Result

Board A ran the server image and board B ran the client image. Each image was
downloaded once with full verify. The 3/3 shape gate and final 20/20 matrix then
used only the two J-Link nRST lines while capturing both CH340 UART streams.

Every final run contained:

```text
RFDBG_SLE_S1_INIT_OK
RFDBG_SLE_S2_SERVER_READY
RFDBG_SLE_S2_SERVER_CONNECTED
RFDBG_SLE_S2_SERVER_DISCONNECTED
RFDBG_SLE_S2_CLIENT_SEEK_READY
RFDBG_SLE_S2_CLIENT_CONNECTED
RFDBG_SLE_S2_CONNECT_DISCONNECT_OK
```

No run contained a missing ROM callback, init/command error, or bounded event
drop. All 20 server logs and all 20 client logs had identical captured byte
counts, which also guards against a marker-only partial lifecycle. The
executable evidence contract is `hil/ws63-sle-connect-reset-matrix.py`. Raw UART
logs and `summary.json` are under
`/private/tmp/ws63-sle-s2-20260807/{3reset,20reset}` on the HIL host.

## Boundary

This is integration and statistical regression evidence for two fixed images,
two boards, and the stated environment. It validates the bounded connection
lifecycle and callback ownership. It is not a mathematical RTOS proof and does
not prove SSAP traffic, future RF environments, or external peers can never
lose events.
