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
| Bounded timeout then successful SHA-1 recovery check | 20/20 nRST |
| Authentication response-2 timeouts | 0 |

The same image was flashed once and exercised for 20 consecutive nRST cycles.
Every boot reached association and DHCP. Public ICMP was 76/100 and gateway
ICMP was 0/100 in that capture window; this remains a separately tracked
AP/network reliability boundary, not a hash/HMAC failure.

At millisecond resolution the eight WPA HMAC requests per boot took 2-5 ms in
total and at most 1 ms per request. The two startup hash requests completed
below the one-millisecond resolution of the diagnostic timer.

The diagnostic timeout-recovery image passed linker-layout, ROM-patch, and
image-hash validation, then completed 20/20 nRST cycles with exactly one
bounded lock-timeout branch followed by one successful SHA-1 known-answer
operation per boot. This proves fail-closed recovery from the synthetic
zero-attempt budget used by the diagnostic feature. It does not claim a real
concurrent-owner or cross-security-domain contention injection.

### Cross-owner contention follow-up

The later `rf-crypto-contention-diag` profile closes the real cross-task owner
gate without changing normal radio images. Two native `hisi-rtos` tasks share
the production `CryptoService` mutex. The holder acquires the service, wakes
the waiter, and explicitly yields at the same priority while retaining the
mutex. The waiter enters the real SPACC AES path and blocks on that mutex. The
holder then completes a SHA-256 known-answer operation, releases the service,
and the waiter completes the AES-128 known-answer operation. Separate counters
prove that contention was observed and both task owners completed.

An initial diagnostic revision held the mutex across `sleep_ms(10)`. On
silicon, the holder remained asleep after the waiter blocked and the main task
waited for completion. Read-only inspection of the immutable image showed
`waiter_attempted=1`, `holder_releasing=0`, and no task completion. That is a
separate all-blocked timed-wake runtime seam. The contention gate now uses an
explicit same-priority yield so this test measures mutex ownership and direct
handoff rather than silently depending on that timer behavior. It does not
claim that the timed-wake seam is fixed.

The implementation is recorded by examples commit `1c2a425`, parent commit
`a207c05e7`, and the deterministic-yield correction `77cdbb255`. Verification
covered 45/45 host tests, RV32 check, and RV32 clippy with warnings denied. The
guarded final link again verified 1,157 layout sections, 4,127 active vendor
relocations, and 37 mask-ROM patches. A 3 MHz full-verify download completed in
93.30 seconds; the first complete boot then passed contention, SAE with
required PMF, EAPOL, DHCP, ARP, gateway/public 5/5 ping, and lease renewal.

The same immutable image then ran 20 nRST trials:

| Evidence | Result |
| --- | --- |
| Cross-task contention observed | 20/20 |
| Holder / waiter completion | 20/20 / 20/20 |
| WPA3 transition association with required PMF | 20/20 |
| Authentication response-2 timeout | 0 |
| TRNG/hash/MAC/cipher/P-256 failure counters | 0 in every run |
| Gateway ICMP | 100/100 |
| Public ICMP | 89/100 |

The matrix command classified 13 runs as full pass, 6 as public-ping degraded,
and 1 as public-ping timeout. Every degraded run had already completed
contention, WPA3, EAPOL, DHCP, and gateway 5/5. Public loss remains the
separately quantified external data-path boundary and is not counted as a
crypto ownership failure. UART captures remain local under
`/private/tmp/ws63-crypto-contention-reset-matrix`; the credential-bearing
ephemeral build directory was removed by the HIL script.

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

1. Run the completed hardware suite against a controlled WPA3-only SAE+PMF BSS.
2. Publish the SVD/PAC and dependent crypto versions in dependency order.
3. Keep standard vectors, software/original-SDK differential evidence, timeout
   and recovery tests, repeated WPA2/WPA3 HIL, and resource measurements for
   every future capability.
