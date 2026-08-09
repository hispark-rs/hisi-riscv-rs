# WS63 BLE U5B Crypto Compatibility Evidence - 2026-08-09

## Scope

This evidence covers the reviewed crypto subset reachable from the pinned WS63
BLE archive: HMAC-SM3, AES-128-CMAC, caller-provided-private-key P-256 public-key
derivation, and P-256 ECDH. It exercises the exact C UAPI compatibility entry
points after the BLE profile installs its unique KM, SPACC, PKE, and TRNG
resources.

It does not prove production random P-256 key generation, injected hardware
fault recovery, pairing/bond persistence, or authenticated two-board pairing.
Those remain U5B/U5C/U5D gates.

## Contract

- KM/keyslot/KLAD and hash/MAC handles are bounded and generation tagged.
- Unknown algorithms, invalid keyslot/destination/engine combinations, stale
  handles, and repeated update calls fail closed.
- Key material is cleared when compatibility state is destroyed or reset.
- SPACC and PKE hardware work executes outside critical sections through the
  installed `hisi-crypto-ws63` service. Hardware failure never selects a
  RustCrypto fallback.
- Random key generation remains rejected until a production DRBG is explicitly
  injected; raw TRNG is not presented as a CSPRNG.

## Known-Answer Vectors

- HMAC-SM3 uses a 20-byte `0x0b` key and message `abc`, checked against an
  independently generated OpenSSL result.
- AES-128-CMAC uses the RFC 4493 one-block vector.
- P-256 uses private scalar 2: public output must equal `2G`, and ECDH with peer
  public key `G` must return the x coordinate of `2G`.

## Verification

- `cargo test --target aarch64-apple-darwin --lib`: 23/23 passed.
- RV32 `cargo clippy -Zbuild-std=core,alloc --lib --features ble-init-diag --
  -D warnings`: passed.
- The release `ble_init_smoke` ELF built through ordinary Cargo/rust-lld.
- ELF SHA-256: `d431abb8fae70a531926feb55d2d2c20337e8a66ec435c55499b7bc839a8961b`.
- probe-rs downloaded the planned binary at 3 MHz with full readback verify in
  57.78 seconds, followed by J-Link hardware nRST.
- Two consecutive hardware boots emitted all required markers:
  `RFDBG_BLE_U5B_CRYPTO_COMPAT_OK`, `RFDBG_BLE_B1_INIT_OK`, and
  `RFDBG_BLE_B2_COMMANDS_OK`.

The second UART capture is retained locally under
`/private/tmp/ws63-ble-u5b-crypto-compat-20260809/uart-reset2.log`. It contains
no credentials and is diagnostic evidence rather than a maintained CI artifact.
