# W2E-H WS63 SPACC Hash And HMAC Evidence

Date: 2026-07-16

## Scope

This evidence closes the SHA-1, SHA-256, HMAC-SHA1, and HMAC-SHA256 part of
W2E-H for the upstream-native WPA2 path. It does not claim that AES key-wrap,
CMAC, SAE P-256, or the complete WPA handshake is hardware accelerated.

The dependency path is:

```text
hostap 2.11 -> hisi-crypto::{TryHash, TryMac}
            -> hisi-crypto-ws63::Ws63Crypto
            -> ws63-pac SPACC registers
```

No vendor hash UAPI, vendor supplicant archive, or LiteOS backend is used. The
RustCrypto implementation remains an explicit host oracle; a SPACC failure is
returned and never selects a software fallback.

## Ownership And Failure Contract

- safe construction consumes the unique HAL `SPACC` token as part of the
  combined `KM`/`SPACC`/`TRNG` crypto service;
- a runtime mutex serializes callers outside critical sections and an internal
  busy guard rejects accidental re-entry;
- channel acquisition, channel clear, and operation completion use separate,
  explicit non-zero polling limits;
- DMA descriptors and message storage are aligned, cache-cleaned before device
  access, and zeroized plus cache-cleaned after every result;
- channel, bus, control, address, and timeout failures are returned explicitly;
- the diagnostic profile exercises the bounded lock-timeout branch and then a
  SHA-1 known-answer operation to prove that the service remains reusable;
- all register access uses the PAC generated from the reviewed SVD model.

## Verification

| Gate | Result |
| --- | --- |
| CMSIS-SVD validation and deterministic PAC regeneration | PASS |
| Host padding, limits, and oversized-input tests | 4/4 PASS |
| RV32 check and clippy `-D warnings` | PASS |
| SHA-1/SHA-256/HMAC-SHA1/HMAC-SHA256 startup KAT | PASS on WS63 |
| Upstream WPA2 association and DHCP | 20/20 nRST |
| Hardware hash requests | 40, zero failures |
| Hardware MAC requests | 160, zero failures |
| Authentication response-2 timeouts | 0 |

The same image was flashed once and exercised for 20 consecutive nRST cycles.
Every boot reached association and DHCP. Public ICMP was 78/100 and gateway
ICMP was 0/100 in that capture window; this remains a separately tracked
AP/network reliability boundary, not a hash/HMAC failure.

At millisecond resolution the eight WPA HMAC requests per boot took 2-5 ms in
total and at most 1 ms per request. The two startup hash requests completed
below the one-millisecond resolution of the diagnostic timer.

The diagnostic timeout-recovery image was built and passed linker-layout, ROM
patch, and image-hash validation. Its final repeated-silicon recovery matrix is
still pending; therefore this page does not yet claim a real contended-channel
fault-injection proof.

## Resource Delta

Against the preceding RKP-PBKDF2-only image:

| Metric | RKP PBKDF2 | SPACC hash/HMAC | Delta |
| --- | ---: | ---: | ---: |
| `.text` | 684344 | 679712 | -4632 bytes |
| `.data` | 14886 | 14886 | 0 bytes |
| `.bss` | 142944 | 147312 | +4368 bytes |
| Total | 842174 | 841910 | -264 bytes |

The `.bss` increase is the current internal aligned descriptor and message
scratch allocation. It is an explicit migration cost, not the final resource
contract: the planned backend API will accept caller-owned static storage so
applications can see and size this RAM at compile time.

Disassembly gives an approximate maximum HMAC call-chain frame reduction from
752 bytes on the RustCrypto path to about 560 bytes on the SPACC path. This is
an observed build result, not a stable ABI guarantee.

## Remaining W2E-H Gate

1. Run the final diagnostic image on silicon and repeat the bounded timeout then
   successful-hash recovery check across the reset matrix.
2. Implement AES block operations, key-wrap, and CMAC through SPACC with explicit
   keyslot ownership.
3. Implement SAE group 19 P-256/Dragonfly through a fallible PKE capability.
4. Keep standard vectors, software/original-SDK differential evidence, timeout
   and recovery tests, repeated WPA2/WPA3 HIL, and resource measurements for
   every capability.
