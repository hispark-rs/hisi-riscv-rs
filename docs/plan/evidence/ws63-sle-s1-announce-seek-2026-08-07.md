# WS63 SLE S1 Announce And Seek Evidence

## Scope

This evidence closes S1: two WS63 boards initialize the shared BGLE controller,
then complete SLE announce and passive seek through a bounded Rust event queue.
It does not claim connection, SSAP, coexistence, pairing UX, or a stable public
SLE API.

## Immutable Inputs

- `ws63-radio-sys`: `530977def3c3dd12ebacd60830812ed8ff814f0a`
- announce runtime source: `hisi-rf-ws63`
  `d4f267b9b9c6292bee0204061092bcfeacc18db8`
- seek matcher source: `hisi-rf-ws63`
  `3013573f559dc75b111ffb488793bc1e7ca12ba4`
- SLE archive profile: `ws63-sle-s0-archive-abi-v1`
- SLE init profile: `ws63-sle-s1-announce-seek-v1`
- announce ELF SHA-256:
  `b26f9dd8bb67b138eb2a8ba646fd91719535496ca8be7e73272996b5b95187b5`
- seek ELF SHA-256:
  `29cc3f6dee815047a01f5b92023ccb478c324252b019fe3bee5b6fb3dda584d3`
- announce FlashPlan image SHA-256:
  `57ed6d9a166dacd8835aa70f398dd1d96c4a6c313ee2d0b4ad1cf3c00e5b3267`
- seek FlashPlan image SHA-256:
  `dc7178b67fbc9e00aac908c22816fa28d2dfa91a65cf427ae9abc636571623c9`
- build features: `sle-init,firmware-example`
- download path: `hisi-fwpkg plan` image followed by probe-rs binary download
  at 3 MHz with complete verify; each image completed in about 67 seconds

The SLE init link profile reuses the silicon-proven shared BGLE controller
archives and callback roots, but roots `enable_sle` and the SLE-specific
announce/seek ABI independently. It does not call `enable_ble` as a bootstrap.

## Software Gates

- `cargo fmt --all -- --check`
- 32/32 `hisi-rf-ws63` host library tests for the SLE feature
- RV32 `cargo clippy` for the library and both examples with `-D warnings`
- RV32 release links of `sle_announce_smoke` and `sle_seek_smoke`
- final ELF undefined-symbol inspection: only the permitted weak
  `sle_at_chba_register` remains
- 2/2 `ws63-radio-sys` SLE ABI tests and workspace clippy
- 3/3 unit tests for `hil/ws63-sle-discovery-reset-matrix.py`
- `ws63-radio-sys` GitHub CI for the fixed init-profile commit

## Silicon Result

Board A ran the announce image and board B ran the seek image. Each image was
downloaded once with full verify. All matrix samples after that used only the
two J-Link nRST lines and captured both CH340 UART streams.

The first matcher used a marker stored only in seek-response data while the
fixture deliberately used passive seek. Both boards initialized and accepted
their announce/seek commands, but the contract reported 0/3 because passive
seek does not guarantee a seek-response payload. This was a fixture predicate
defect, not an SLE init failure. The matcher was corrected to use the fixture's
fixed announce address, after which the same announce image and corrected seek
image passed the 3/3 shape gate and final 20/20 matrix.

Every final run contained:

```text
RFDBG_SLE_S1_INIT_OK
RFDBG_SLE_S1_ANNOUNCE_OK
RFDBG_SLE_S1_SEEK_READY
RFDBG_SLE_S1_SEEK_MATCH
```

No final run contained a missing ROM callback, init/command error, or bounded
event drop. The executable evidence contract is
`hil/ws63-sle-discovery-reset-matrix.py`. Raw UART logs and `summary.json` are
under `/private/tmp/ws63-sle-s1-20260807/{3reset-v2,20reset}` on the HIL host.

## Boundary

This is integration and statistical regression evidence for two fixed images,
two boards, and the stated environment. It validates SLE init, announce, seek,
callback copying, and bounded event ownership. It is not a mathematical RTOS
proof and does not prove that future SLE connection traffic or RF environments
can never lose events.
