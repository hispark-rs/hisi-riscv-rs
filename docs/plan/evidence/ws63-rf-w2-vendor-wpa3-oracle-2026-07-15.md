# W2 Vendor WPA3 Oracle

Date: 2026-07-15

## Scope

This evidence isolates the transitional vendor WPA3-Personal archive on a
controlled WPA2/WPA3 transition BSS. It is a migration oracle for the pinned
upstream hostap path. It does not close the upstream-native WPA3 gate and does
not make the vendor archive a long-term dependency.

The archive under test had SHA-256
`d1bfa6a87459d16efdea4c796ed8af43e678c48e4570ce041218312ff9952778`.
Credentials were injected through an ephemeral build environment and are not
part of this evidence or the repository.

## Root Cause And Fix

The first SAE attempts consistently ended with vendor status 15. Buffered
vendor diagnostics showed `hash start unregister` before the failure. In the
WS63 SDK, `uapi_drv_cipher_env_init()` creates the unified-cipher environment,
while the separate `mbedtls_adapt_register_func()` call populates the mbedTLS
harden function tables. The original Rust initialization called only the first
function, leaving the SHA provider used by SAE unregistered.

The Rust vendor-oracle path now follows the SDK ordering:

1. initialize the unified-cipher environment;
2. register the mbedTLS harden providers for the WPA3 profile;
3. initialize Wi-Fi and start the station.

Registration failure is reported as a distinct `CryptoInitialize` error. The
WPA2-only profile does not reference the harden registration symbol.

## Silicon Results

The first fixed image retained vendor diagnostics in a fixed circular buffer
and flushed them only after association, so UART output could not extend a
scheduler-lock interval. A verified 3 MHz probe-rs download completed in
76.57 seconds. The image then completed:

- SAE Authentication sequences 1 and 2;
- association and the WPA four-way handshake;
- DHCP and neighbor discovery;
- 5/5 gateway ICMP replies and 5/5 public ICMP replies;
- DHCP renewal while the long-lived network runner remained alive.

The log no longer contained `hash start unregister`.

A second, normal non-verbose image was downloaded with full verification at
1 MHz in 129.95 seconds. It independently completed WPA3 association, DHCP,
5/5 gateway replies, 3/5 public replies, and DHCP renewal. The public loss is
recorded as network-path evidence, not an authentication failure.

## Probe Speed Boundary

The 3 MHz transport is not yet a stable default. One full verified download
succeeded, but the next attempt timed out while programming the page at
`0x002b0000`; its immediate 1 MHz retry could not reconnect until a hardware
nRST. After nRST, a 1 MHz probe-rs SRAM read succeeded and the subsequent full
download passed. Connectivity smoke therefore remains at its conservative
speed until repeated 3 MHz download and recovery evidence is available.

## Remaining Gate

- reproduce transition-mode SAE/PMF through the upstream-native hostap path;
- run a pure WPA3-only SAE/PMF fixture;
- keep vendor archive and LiteOS behavior as an oracle for one migration
  release only;
- close the per-capability hardware-crypto gate before stabilizing WPA3.
