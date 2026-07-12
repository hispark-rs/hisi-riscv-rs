# WS63 WPA2-Personal Oracle Evidence - 2026-07-12

## Scope

This records the first protected-network connectivity proof on a WS63 EVB. The
firmware links the delivered full supplicant and SDK-built mbedTLS/security
archives, so it is an oracle for the later WPA2-only extraction, not the final
component or resource boundary.

The test passphrase is injected at build time through `WS63_WIFI_PASSPHRASE` and
is intentionally absent from source, logs, and this evidence file. The resulting
test firmware contains the credential and must not be published as an artifact.

## Silicon Markers

Network: WPA2-Personal `HUAWEI-HLJ_Guest`. Target: `1.1.1.1`.

```text
RF2_INIT_OK ifname=wlan0
RF3_AP ssid=HUAWEI-HLJ_Guest freq=0x0000096c
RF5B_WPA_CONNECT_OK freq=0x0000096c
RF5A_DHCP_OK addr=192.168.155.2 prefix=0x00000018 router=192.168.155.1
RF5A_ARP_OK rx=0x00000004
RF5C_PING_BEGIN target=1.1.1.1 via=192.168.155.1
RF5C_PING_OK rx=0x00000005
```

## Defects Closed By The Run

- Public Wi-Fi structs use the SDK's `--short-enums` ABI. The checked sizes are
  52 bytes for scan AP info, 111 for association request, and 176 for events.
- The security engine is initialized before registering the mbedTLS harden
  hash/AES/ECP provider, matching the vendor startup order.
- Crypto allocations support the required 32-byte alignment and delegate cache
  maintenance to the HAL.
- Connection completion uses the public event callback. It does not poll the
  vendor status API, whose pointer-to-decimal control message adds an unnecessary
  formatter and temporary-semaphore boundary.

## Remaining Boundary

This evidence does not validate WPA3/SAE, SoftAP, Enterprise, WPS, P2P, or WAPI.
The next gate is a WPA2-PSK/CCMP-only archive plus shared known-answer tests for
the WS63 ROM/hardware provider and the RustCrypto fallback.
