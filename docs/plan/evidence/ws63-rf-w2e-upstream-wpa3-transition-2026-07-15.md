# W2E Upstream WPA3 Transition Evidence

Date: 2026-07-15

## Scope

This evidence proves that the pinned upstream hostap 2.11 path can complete
SAE on a controlled WPA2/WPA3 transition BSS. It covers the native
`os_hisi_rtos` / `eloop_hisi_rtos` / `driver_ws63` path; it does not use the
vendor supplicant archive or a LiteOS backend.

Credentials were injected through a no-echo environment and an ephemeral
`CARGO_TARGET_DIR`. The temporary build and UART capture directory was removed
when the HIL script exited. No credential or credential-derived image is stored
in the repository.

## Root Cause

The first native transition attempt submitted an association request, but
hostap never received `EVENT_EXTERNAL_AUTH`. The Rust callback modeled the
vendor `ext_external_auth_stru` as 32 bytes and rejected every payload whose
length differed.

The vendor SDK source declares `auth_action` as an enum, but the delivered WS63
toolchain uses a short-enum ABI. The final vendor firmware disassembly is the
decisive oracle: `cfg80211_external_auth_req` invokes event 13 with a 28-byte
payload. The on-wire RV32 layout is:

- `auth_action: u8` at offset 0;
- `bssid[6]` at offset 1;
- `ssid` at offset 8;
- `ssid_len` at offset 12;
- `key_mgmt_suite` at offset 16;
- `status` at offset 20;
- `pmkid` at offset 24;
- total size 28 bytes.

The fix applies that layout in both directions: the firmware-to-host external
auth event and the host-to-firmware external-auth status ioctl. Compile-time
RV32 size and offset assertions now guard the contract. The callback performs
only bounded validation, deep copy, diagnostics and wakeup; UART output remains
outside callback context.

## Verification

Host and static gates passed:

- 36 `ws63-rf-rs` upstream-WPA3 host tests;
- clippy with warnings denied;
- both native supplicant profiles and their external ABI gate;
- the complete `ws63-radio-sys` host workspace test suite;
- RV32 `wifi_init_smoke` check with the upstream-WPA3 feature set;
- 1157-section final RF layout verification and 37 generated ROM patches.

The first 3 MHz download attempt timed out during page programming. The board
was recovered with hardware nRST and the same image was downloaded at 1 MHz
with full verification in 142.10 seconds. The conservative connectivity-smoke
default therefore remains 1 MHz.

The real-silicon run produced the required markers for:

- transition-mode RSNE selection;
- upstream-native SAE association with PMF required;
- connected event delivery;
- DHCP lease and neighbor-cache resolution;
- 5/5 gateway and 5/5 public ICMP replies with zero RX queue drops;
- long-lived network-runner steady state and DHCP renewal;
- no fatal connectivity marker.

`hil/ws63-connectivity-smoke.sh` completed with
`WS63 CONNECTIVITY SMOKE: PASS`.

## Remaining Gate

- repeat transition-mode HIL across the reset matrix and quantify failures;
- run SAE plus required PMF against a controlled WPA3-only BSS;
- retain the vendor path as an oracle for one migration release only;
- close the W2E-H per-capability hardware-crypto gate before stabilizing WPA3.

