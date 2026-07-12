# WS63 WPA2-Personal Cropped Supplicant Evidence - 2026-07-12

## Scope

This is the first on-silicon proof for the independently rebuilt
`wifi-wpa2-personal` profile. It uses the SDK 1.10.106 compiler command graph,
but excludes SAE, ECC, OWE, AP, EAP-TLS/server, WPS, WAPI, 11r and MBO sources.

The test credential was injected through `WS63_WIFI_PASSPHRASE`; neither the
credential nor the resulting firmware is published.

## Build Closure

- 52 selected vendor C sources plus two repository-owned compatibility units.
- Archive size: 6.9 MiB versus 13 MiB for the delivered full archive.
- Canonical app image: 535,792 bytes versus 656,584 bytes for the full oracle,
  a reduction of 120,792 bytes.
- 1,683 final RF layout sections verified, 5,827 vendor relocations patched and
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

## Remaining Boundary

The supplicant source boundary is now cropped, but the image still links the
SDK mbedTLS and unified-cipher archives. W1 must replace that broad dependency
with the explicit WS63 hardware/ROM provider plus common Rust known-answer
tests. WPA3/SAE, SoftAP and Enterprise remain separate later gates.
