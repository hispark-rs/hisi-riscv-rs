# WS63 WPA2-Personal Cropped Supplicant Evidence - 2026-07-12

## Scope

This is the first on-silicon proof for the independently rebuilt
`wifi-wpa2-personal` profile. It uses the SDK 1.10.106 compiler command graph,
but excludes SAE, ECC, OWE, AP, EAP-TLS/server, WPS, WAPI, 11r and MBO sources.

The test credential was injected through `WS63_WIFI_PASSPHRASE`; neither the
credential nor the resulting firmware is published.

## Build Closure

- 50 selected vendor C sources plus two repository-owned compatibility units.
- Archive size: 6.7 MiB versus 13 MiB for the delivered full archive.
- Canonical app image: 519,020 bytes versus 656,584 bytes for the full oracle,
  a reduction of 137,564 bytes.
- 1,486 final RF layout sections verified, 5,335 vendor relocations patched and
  37 mask-ROM patches generated.
- Final ELF contains no SAE, dragonfly, EAP-TLS/server, WPS or WAPI
  implementation. AP entry points are fail-closed stubs because the vendor STA
  task and event objects reference the combined HostAP binary unconditionally.

SDK 1.10.106 requires `CONFIG_WPA3` and `CONFIG_HOSTAPD_WPA3` for a shared
RSN/PMK compile shape even when SAE and AP implementations are absent. Removing
those defines produced repeatable authentication failure `0x1451`; preserving
the defines without the feature sources restored WPA2. They are therefore
vendor compatibility defines, not a claim that this profile implements WPA3 or
SoftAP.

## Silicon Markers

Network: WPA2-Personal `HUAWEI-HLJ_Guest`. Target: `1.1.1.1`.

```text
RF2_INIT_OK ifname=wlan0
RF3_SCAN_OK count=0x0000001e
RF5B_WPA_CONNECT_OK freq=0x0000096c
RF5A_DHCP_OK addr=192.168.155.2 prefix=0x00000018 router=192.168.155.1
RF5A_ARP_OK rx=0x00000004
RF5C_PING_OK rx=0x00000005
```

The completed W1 run removed both SDK mbedTLS archives from the supplicant
image. Rust calls the published WS63 `uapi_drv_cipher_pbkdf2`, converts the
resulting 32-byte PMK to the vendor API's 64-byte hexadecimal PSK form, and
uses RustCrypto SHA-1/SHA-256, HMAC-SHA1/HMAC-SHA256 and AES block primitives.
The provider passes IEEE/RFC HMAC, PBKDF2 and AES known-answer tests before
association and reproduced all connectivity markers on real WS63 silicon.

Direct SPACC HMAC was also exercised: clear-key loading required the SDK's
`--short-enums` ABI, then the engine reached `ERROR_SECURITY_HASH_CALC_TIMEOUT`
in the transitional bare-metal runtime. It is therefore not silently selected.
It remains an experimental future `hisi-crypto` backend until clock, IRQ and
wait semantics have their own HIL gate.

## 2026-07-17 Native Runtime Lock Regression

Parent commit `9369b7828` removed the final no-op scheduler-lock assumption
from the vendor ABI adapter: `LOS_TaskLock` and `LOS_TaskUnlock` now use the
same nested `hisi-rf-rtos-driver` contract as `osal_kthread_lock` and
`osal_kthread_unlock`. A host mock-runtime test proves that both lock and
unlock calls cross that contract. Parent commit `389f8e369` also restored the
standalone `wifi-wpa2-personal` build after P-256 became an explicitly gated
`upstream-supplicant-wpa3` capability; CI now builds both security profiles
independently.

The committed vendor-WPA2 profile was then rebuilt and run on WS63:

- 1,476 final layout sections, 5,322 patched relocations and 37 ROM patches;
- 3 MHz full-verify download completed in 68.32 seconds;
- association, DHCP, public ICMP `5/5`, RX queue drop `0` and DHCP renew all
  passed;
- gateway ICMP remained `0/5`, matching the separately frozen AP environment
  boundary rather than a radio/runtime regression;
- the smoke script reported `WS63 CONNECTIVITY SMOKE: PASS` with no fatal
  connectivity or panic marker.

One preceding programming attempt failed in probe-rs before firmware execution
(page-program timeout followed by DMI reconnect failure). The board was
restored with the complete official FWPKG and its normal boot markers were
verified before the successful retry. That transport failure is deliberately
excluded from the WPA2 behavior result. Credentials and the temporary firmware
build directory were not retained.

## Remaining Boundary

WPA2-Personal/CCMP no longer links SDK mbedTLS. Application TLS is a separate
layer: its planned default remains mbedTLS, with `embedded-tls` as an optional
backend. WPA3/SAE, SoftAP and Enterprise remain separate later gates.
