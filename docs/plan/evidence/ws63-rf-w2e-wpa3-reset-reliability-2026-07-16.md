# W2E Upstream WPA3 Reset Reliability Evidence

Date: 2026-07-16

## Scope

This evidence closes the repeated-reset reliability gate for the pinned upstream
hostap 2.11 path on a controlled WPA2/WPA3 transition BSS. It uses the native
`os_hisi_rtos` / `eloop_hisi_rtos` / `driver_ws63` path and the explicit
RustCrypto-plus-WS63-TRNG crypto profile. It does not use the vendor supplicant
archive or a LiteOS backend.

Credentials were injected through a no-echo environment and an ephemeral build
directory. No credential, credential-derived image, or raw UART capture is stored
in the repository.

This result does not prove operation against a WPA3-only BSS and does not claim
that the WPA handshake is fully hardware accelerated.

## Failure Classification

The first 20-reset diagnostic matrix completed 18 associations and failed twice.
Both failures had zero `WLAN_AUTH_RSP2_TIMEOUT` events and occurred after SAE but
before the four-way handshake:

- one run repeatedly received vendor association status `8030`, normalized to
  IEEE status 30, without a comeback interval;
- one run received an association-success event but no first EAPOL notification
  or frame before the outer connect deadline.

The event ring remained bounded with no drops, and the HAL TRNG capability
reported no failures. The remaining fault was therefore an association-recovery
gap, not an SAE response timeout, entropy failure, event-queue overflow, or
EAPOL MIC failure.

## Fix

The submitted recovery path is split across the chip integration and safe Rust
adapter:

- parent commit `aecb5742e` feeds the firmware scan result set into hostap's BSS
  cache, treats vendor disconnect/status events as recoverable until the overall
  connect deadline, records bounded association/EAPOL diagnostics, and performs
  a bounded nonblocking EAPOL receive fallback;
- `ws63-radio-sys` commit `fa3de42` adds a three-second first-EAPOL watchdog. It
  cancels hostap's generic blacklist-producing authentication timeout, requests
  an asynchronous disconnect, and reassociates the explicitly selected cached
  BSS after the disconnect event. A one-second fallback covers firmware that
  omits the disconnect callback;
- `hisi-crypto-ws63` commit `efe7bd4` consumes the unique HAL TRNG peripheral
  token through `Ws63Entropy`, removing the legacy implicit global TRNG UAPI and
  its publication race;
- examples commit `0b7a5e3` exposes only non-secret bounded diagnostics needed to
  classify future association failures.

The retry remains bounded by the caller's existing connect deadline. It does not
increase the global timeout, busy-wait in the executor, invoke user code from a
callback, or silently switch crypto backends.

## Verification

The final submitted image was flashed once and exercised with 20 consecutive
hardware nRST cycles. The matrix reported:

| Evidence | Result |
| --- | --- |
| Image/init/scan completed | 20/20 |
| Upstream-native transition-mode WPA3 association | 20/20 |
| DHCP completed | 20/20 |
| Authentication response-2 timeouts | 0 |
| Captured gateway ICMP replies | 70/70 |
| Connect failures | 0/20 |

Association completed between 7.788 and 8.064 seconds after reset in this final
matrix. The matrix used one immutable image and nRST only, so download failures
or image changes are not mixed into the reliability result.

Before the final matrix, targeted A/B matrices independently reproduced the two
tail failures and demonstrated that generic blacklist removal alone, disconnect
cleanup alone, or EAPOL polling alone was insufficient. Reliability reached
20/20 only after the selected BSS was retained in hostap's cache and the
first-EAPOL recovery could reassociate that exact BSS after asynchronous
disconnect completion.

## Remaining Gate

- run SAE plus required PMF against a controlled WPA3-only BSS;
- retain the vendor path as an oracle for one migration release;
- close the W2E-H per-capability hardware-crypto gate before stabilizing WPA3;
- keep the 20-reset transition matrix as a regression gate for subsequent W2
  changes.
