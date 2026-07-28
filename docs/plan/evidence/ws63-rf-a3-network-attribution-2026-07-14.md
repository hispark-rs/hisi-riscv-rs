# WS63 RF A3 Network Attribution Evidence (2026-07-14)

## Scope

This evidence closes the remaining A3 network-attribution gate. It distinguishes
loss above and below the Rust RX seam, measures the bounded RX queue, and compares
the WS63 result with a host forced through the same Guest AP interface.

It does not claim that the Guest network is lossless or that the bring-up-only IP
helper is a production network stack. Sustained lease, neighbor-cache and traffic
behavior remains A4 work.

## Frozen Inputs

| Input | Revision or SHA-256 |
| --- | --- |
| parent implementation | `501d80365` |
| WS63 examples | `100bf3e78` |
| final `wifi_init_smoke` ELF | `85d831e2a71d2e8d649d80d2d526edbc17f1e0f1e976a56627ebc5ade9a7eb3e` |
| canonical `.hisi.img` | `97a3b09b1bd6457d1b8e67bc529b977a7c8dd6b20758a9e99be6429db29f5e9d` |
| FlashPlan JSON | `c6e62c99a1879e88acc3d5f2b8241f19413c228a4b1917414233d2a26d5477b3` |
| five-reset summary | `0303e9973b3d631b52249ffed6053b3e3a0fff85f681bb8fc28323b5d7e4b8ec` |

The WPA2-Personal passphrase was injected at build time and is not stored in this
evidence. Raw UART logs remain under the ignored local
`target/hil/ws63-connectivity-echo-seam-current-5x-20260714/` directory because
scan output contains nearby AP identifiers.

## Rust RX Seam

The RF bridge now counts queue-full drops, queue high-water, and diagnostic ICMP
Echo Reply identifiers/sequences at `driverif_input`, before smoltcp consumes the
frame. The current guarded image passed layout verification for 1,485 sections,
patched 5,334 vendor relocations, generated 37 ROM patches, and then passed
probe-rs bin download with full readback verification.

Five physical J-Link nRST cycles produced:

| Observation | Result |
| --- | ---: |
| init/scan/WPA2/DHCP/ARP | 5/5 |
| `WLAN_AUTH_RSP2_TIMEOUT` (`0x1451`) | 0/5 |
| gateway replies | 0/25 |
| public `1.1.1.1` replies | 21/25 |
| RX queue-full drops | 0 |
| maximum RX queue occupancy | 1/4 |
| replies seen by app but not at RF seam | 0 |
| replies seen at RF seam but not by app | 0 |

Every missing public reply was already absent at the vendor-to-Rust seam. The
Rust queue and consumer therefore did not discard or misclassify those packets.
This does not distinguish RF/air/AP/uplink loss below that seam, but it rules out
the suspected four-frame RX ring overflow for this workload.

## Same-AP Reference Host

The Mac had multiple active networks, so the comparison explicitly bound both
source and interface:

```text
ping -b en0 -S 192.168.155.9 -c 20 192.168.155.1
ping -b en0 -S 192.168.155.9 -c 100 -i 0.2 1.1.1.1
```

An interface-scoped route lookup independently resolved both targets through
Guest gateway `192.168.155.1` on Wi-Fi `en0`.

| Target | Host result | WS63 20-reset baseline |
| --- | ---: | ---: |
| Guest gateway `192.168.155.1` | 0/20 | 0/100 |
| public `1.1.1.1` | 88/100 (12% loss) | 88/100 (12% loss) |

The gateway result is an AP policy/environment property, not evidence that WS63
ARP or ICMP construction is broken. The identical public-loss point estimate,
together with zero queue drops and exact seam/app parity, establishes a quantified
environmental boundary for the A3 capability proof. It is not a promise that all
future loss has the same cause; a changed AP, payload, route, or traffic profile
must be measured again.

## A3 Decision

A3 is closed for the pinned payload and test environment:

- scheduler invariants and the 20-reset connection matrix are stable;
- Q3 has an archive-bound task profile and Q4 has an explicit no-quota decision;
- authentication, DHCP and ARP complete deterministically in the measured matrix;
- the remaining ICMP loss is below the Rust RX seam and reproduced by a same-AP
  reference host at the same 12% rate;
- gateway ICMP silence is reproduced by that reference host.

A4 now owns sustained-IP work: a long-lived smoltcp or embassy-net runner, lease
renewal, neighbor-cache lifecycle and repeated traffic over longer windows.
