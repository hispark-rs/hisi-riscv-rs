# WS63 RF A4 Wi-Fi Vertical Slice Evidence (2026-07-14)

## Scope

This evidence closes the first on-silicon A4 vertical slice:

- chip-neutral `hisi-rf` owns `RadioController`, `RadioRunner`,
  `WifiController`, `WifiDevice`, validated Wi-Fi config and a bounded event
  queue;
- the WS63 adapter remains in transitional `ws63-rf-rs` and is the only layer
  that knows vendor scan/auth fields;
- the application owns one long-lived smoltcp interface, DHCP socket, neighbor
  cache and ICMP socket;
- the IP stack does not move into `hisi-rf`.

Pinned revisions:

- parent: `fc95e7d0f08d83bb961eee744d3575ad1173d381`;
- `hisi-rf`: `44f581387eb82fc6d2df98329faeb37b97ccd53a`;
- `ws63-examples`: `5a900f78c281f6e5abb3f18101f4de5ba202f7a4`;
- WPA2-only archive SHA-256:
  `891c195279d768ce5664e16fecac95f353fd2217c4e3590315baa4cb6e7f25a2`.

## Build And Link Gate

The guarded build used the pinned WPA2-only archive and a secret injected by
environment. It completed the stock-rust-lld layout pass, vendor relocation
transform, final layout verification and mask-ROM patch generation:

- verified layout sections: 1,486;
- patched vendor relocations: 5,337;
- generated mask-ROM patches: 37;
- code-area hash:
  `c0adac6db87ecc721c5fa661f1b95d3ce1c79c4e481d85972b83479459b797b8`.

Final artifacts:

- ELF SHA-256:
  `be5870b687885ea56218924529b5660f9e2f32a3a3c0af84d130dfd9608c056e`;
- planned image SHA-256:
  `56655ee8d98b5f5b954b867269314108ff49085e65a2e138d7de7c457e278319`;
- FlashPlan base `0x00230000`, body length 570,940 bytes, erase length
  571,708 bytes.

The target matrix passed for WPA2 A4, legacy open-network full-init and RF1-only
builds. WPA2 A4 also passed clippy with `-D warnings`; the WS63 bridge host suite
passed 17/17, including DHCP direction parsing and bounded RX queue tests.

## HIL Result

The final run used the complete planned bin image, probe-rs verify, J-Link nRST
and UART at 115200. A 2,000 kHz attempt hit a page-program timeout at
`0x00240000`; the accepted run used 1,000 kHz and completed download/verify in
32.33 seconds. The failed transport attempt is not counted as firmware evidence.

Accepted UART facts:

- `RF2_INIT_OK ifname=hisi-rf`;
- bounded events: initialized, scan-completed and connected;
- WPA2 association to the Guest AP succeeded;
- DHCP acquired `192.168.155.8/24`, router `192.168.155.1`;
- smoltcp neighbor discovery produced
  `RF5A_ARP_OK mode=smoltcp-neighbor-cache`;
- gateway ICMP: 0/5, matching the established AP policy boundary;
- public `1.1.1.1`: 5/5, RTT 268--314 ms, RX queue drop 0;
- the runner entered `A4_NET_RUNNER_STEADY` and emitted continuing alive markers;
- after the smoke-only 20-second lease cap, the L2 seam observed one additional
  DHCP client REQUEST and one server response:
  `A4_DHCP_RENEW_OK client=1 server=1`.

smoltcp intentionally emits `Configured` only when the effective lease
configuration changes. Therefore renewal evidence comes from versioned L2 packet
counters, not from incorrectly expecting a second configuration event.

## Multi-Network Host Boundary

The comparison Mac simultaneously had Wi-Fi `en0` (`192.168.155.9/24`), USB
Ethernet `en7` (`192.168.3.2/24`), and VPN routes. The ordinary default route was
therefore not valid Guest-AP evidence. Host comparisons must bind both interface
and source address:

```bash
route -n get -ifscope en0 1.1.1.1
ping -b en0 -S 192.168.155.9 1.1.1.1
```

The interface-scoped route resolved through `192.168.155.1` on `en0`. The frozen
A3 100-packet comparison remains the statistical environment boundary; this A4
run proves architectural parity, not a new universal packet-loss claim.

## Remaining A4 Closeout

`hisi-rf 0.1.0-alpha.1` is published on crates.io. The transitional
`ws63-rf-rs::radio` facade is deprecated through the parent 0.7.x train and is
removed no earlier than parent v0.8.0.

The new `hil/ws63-connectivity-smoke.sh` entry was then run locally against the
same board and AP. It rebuilt the guarded image, downloaded and verified it in
32.74 seconds at 1,000 kHz, captured UART after J-Link nRST, passed every
control/L2/IP/renew assertion, and printed `WS63 CONNECTIVITY SMOKE: PASS`.

GitHub workflow dispatch
[29326512586](https://github.com/hispark-rs/hisi-riscv-rs/actions/runs/29326512586)
was cancelled while queued because the repository currently has no registered
`ws63-hil` self-hosted runner. This is an infrastructure gap, not firmware
evidence. A4 remains open until that runner executes the same committed script
and records the first CI PASS URL.
