# W2E-H WS63 RKP PBKDF2 Evidence

Date: 2026-07-16

## Scope

This evidence closes only the first W2E-H capability: WPA-Personal PMK
derivation through WS63 RKP PBKDF2-HMAC-SHA1. It does not claim that SHA/HMAC,
AES/CMAC/key-wrap, SAE P-256, or the complete WPA handshake is hardware
accelerated.

The upstream-native supplicant reaches the engine through this dependency path:

```text
hostap 2.11 -> hisi-crypto::Pbkdf2HmacSha1
            -> hisi-crypto-ws63::Ws63Crypto
            -> ws63-pac KM/RKP registers
```

No vendor PBKDF2 UAPI, vendor supplicant archive, or LiteOS backend is required.
The software RustCrypto provider remains an explicit host oracle; hardware
failure never selects it as an implicit fallback.

## Ownership And Failure Contract

- the combined WS63 crypto service consumes the unique HAL `KM`, `SPACC`, and
  `TRNG` peripheral tokens; this PBKDF2 slice exercises `KM`/RKP and `TRNG`;
- one runtime mutex serializes hardware use outside critical sections;
- a second in-object guard rejects accidental re-entry;
- RKP lock acquisition and each output-block operation have explicit non-zero
  poll limits;
- hardware KDF errors and TRNG failures are returned to the supplicant;
- password, salt, intermediate state, output registers, and local temporary
  buffers are cleared after every result;
- the implementation uses the PAC generated from the SVD-modeled RKP register
  block rather than raw MMIO constants.

## Verification

The following gates passed on the submitted implementation:

| Gate | Result |
| --- | --- |
| CMSIS-SVD validation and PAC regeneration | PASS |
| Register-access policy audit | PASS |
| Host unit vectors | 3/3 PASS |
| RV32 check and clippy `-D warnings` | PASS |
| PBKDF2 public-vector startup KAT | PASS on WS63 |
| Upstream WPA2 association and DHCP | 20/20 nRST |
| Hardware PBKDF2 requests per boot | 2 |
| Hardware PBKDF2 failures | 0/40 |
| Authentication response-2 timeouts | 0 |

The same image was flashed once and exercised for 20 consecutive nRST cycles.
Every boot reached upstream-native WPA2 association and DHCP. Gateway ICMP was
0/100 and public ICMP was 84/100 in that capture window; this is retained as the
known AP/network reliability boundary and is not counted as a key-derivation or
handshake failure.

The final diagnostic run reported 13 ms total and 13 ms maximum for the two
hardware PBKDF2 calls at millisecond timer resolution. The complete 3 MHz
probe-rs download, including read-back verification, took 76.11 seconds.

## Resource Delta

Against the prior upstream-WPA2 RustCrypto PBKDF2 ELF:

| Metric | RustCrypto baseline | RKP PBKDF2 | Delta |
| --- | ---: | ---: | ---: |
| `.text` | 684760 | 684344 | -416 bytes |
| `.data` | 14690 | 14886 | +196 bytes |
| `.bss` | 143456 | 142944 | -512 bytes |
| Total | 842906 | 842174 | -732 bytes |

Disassembly shows the old `pbkdf2_sha1` path reserving 768 bytes of stack. The
new C shim does not reserve a frame before entering the Rust service, while the
hardware service path reserves 512 bytes, reducing the observed maximum frame
for this call chain by about 256 bytes.

## Remaining W2E-H Gate

1. SHA-1/SHA-256 and HMAC-SHA1/HMAC-SHA256 through SPACC.
2. AES block operations, AES key-wrap and CMAC through SPACC/keyslot ownership.
3. SAE group 19 P-256/Dragonfly through a fallible PKE capability.
4. Standard vectors, software/original-SDK differential evidence, timeout and
   recovery tests, repeated WPA2/WPA3 HIL, and resource measurements for every
   capability above.
